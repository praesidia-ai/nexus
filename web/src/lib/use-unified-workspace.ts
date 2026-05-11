"use client";

import { useState, useRef, useCallback, useEffect } from "react";
import { api, BASE, streamChat, type ChatEvent } from "@/lib/api";
import type {
  TimelineItem,
  WorkspaceState,
  WorkspaceMode,
  PlanData,
  AgentRoleType,
  AgentRunStatus,
  PipelineMode,
} from "@/lib/unified-types";
import type { WavePipelineState } from "@/lib/wave-pipeline-types";

/** SSE event from the standard agent loop */
interface AgentEvent {
  type: string;
  content?: string;
  agent?: AgentRoleType;
  tool?: string;
  name?: string;
  arguments?: Record<string, unknown>;
  output?: string;
  success?: boolean;
  duration_ms?: number;
  number?: number;
  max?: number;
  intent?: string;
  operations?: { file?: string; action?: string; description?: string }[];
  risk_level?: string;
  warnings?: string[];
  phase?: string;
  status?: string;
  change?: {
    change_type?: string;
    path?: string;
    agent?: string;
  };
  result?: {
    check_type?: string;
    passed?: boolean;
  };
  strategy?: string;
  retry_count?: number;
  decision?: {
    agent?: string;
    decision?: string;
    reasoning?: string;
  };
  message?: string;
  pct?: number;
  summary?: string | {
    status?: string;
    files_created?: string[];
    files_modified?: string[];
    total_iterations?: number;
  };
  iterations?: number;
  tools_used?: number;
  fatal?: boolean;
}

/** SSE event from the wave pipeline */
interface WaveEvent {
  type: string;
  total_waves?: number;
  total_agents?: number;
  wave_index: number;
  wave_label?: string;
  agent_count?: number;
  agent_id: string;
  agent_label?: string;
  agent_icon?: string;
  agent_color?: string;
  number?: number;
  max?: number;
  success?: boolean;
  summary?: string | {
    status?: string;
    agents_succeeded?: number;
    total_agents_run?: number;
    files_created?: string[];
    files_modified?: string[];
    duration_ms: number;
    total_iterations?: number;
  };
  duration_ms: number;
  files_changed?: number;
  agents_succeeded: number;
  agents_failed: number;
  content?: string;
  message?: string;
  fatal?: boolean;
}

let idCounter = 0;
function nextId() {
  return `tl-${++idCounter}-${Date.now()}`;
}

export function useUnifiedWorkspace(projectId: string) {
  const [state, setState] = useState<WorkspaceState>({
    mode: "auto",
    timeline: [],
    streaming: false,
    agentRunning: false,
    activeAgents: {},
    processingSteps: [],
  });

  const abortRef = useRef<AbortController | null>(null);
  const currentAssistantId = useRef<string | null>(null);
  const projectModelLoaded = useRef(false);
  const loadedProjectId = useRef<string | null>(null);
  // Use refs to avoid stale closures in streaming callbacks
  const stateRef = useRef(state);
  stateRef.current = state;

  // Switching projects must fully reset the workspace — otherwise the
  // previous project's timeline, active agents, and streaming flags bleed
  // into the new project's view. Abort any in-flight stream too.
  useEffect(() => {
    if (loadedProjectId.current === projectId) return;
    if (abortRef.current) {
      abortRef.current.abort();
      abortRef.current = null;
    }
    currentAssistantId.current = null;
    projectModelLoaded.current = false;
    loadedProjectId.current = projectId;
    setState({
      mode: "auto",
      timeline: [],
      streaming: false,
      agentRunning: false,
      activeAgents: {},
      processingSteps: [],
    });
  }, [projectId]);

  // Load per-project model + existing conversation + plan
  useEffect(() => {
    if (projectModelLoaded.current) return;
    projectModelLoaded.current = true;

    api.getProjectModel(projectId).then((m) => {
      setState((s) => ({
        ...s,
        provider: m.provider ?? undefined,
        model: m.model ?? undefined,
      }));
    }).catch(() => {});

    // Load existing conversation
    api.listConversations(projectId).then(async (convs) => {
      if (convs.length > 0) {
        const latest = convs[convs.length - 1];
        const msgs = await api.listMessages(projectId, latest.id);
        if (msgs.length > 0) {
          const items: TimelineItem[] = msgs.map((m) => ({
            id: m.id,
            type: m.role === "user" ? "user_message" as const : "assistant_message" as const,
            timestamp: new Date(m.created_at).getTime(),
            content: m.content,
          }));
          setState((s) => ({ ...s, conversationId: latest.id, timeline: items }));
        }
      }
    }).catch(() => {});

    // Load existing plan
    api.getPlan(projectId).then((res) => {
      if (res.exists && res.plan) {
        setState((s) => ({ ...s, currentPlan: res.plan as PlanData }));
      }
    }).catch(() => {});
  }, [projectId]);

  // --- Send a chat message ---
  const sendMessage = useCallback(async (text: string) => {
    if (!text.trim()) return;

    const userItem: TimelineItem = {
      id: nextId(),
      type: "user_message",
      timestamp: Date.now(),
      content: text.trim(),
    };
    const assistantItem: TimelineItem = {
      id: nextId(),
      type: "assistant_message",
      timestamp: Date.now(),
      content: "",
      streaming: true,
    };

    currentAssistantId.current = assistantItem.id;

    setState((s) => ({
      ...s,
      streaming: true,
      processingSteps: [],
      timeline: [...s.timeline, userItem, assistantItem],
    }));

    abortRef.current = new AbortController();

    try {
      // Read from ref to avoid stale closure
      const convId = stateRef.current.conversationId;
      const provider = stateRef.current.provider;
      const model = stateRef.current.model;

      await streamChat(
        projectId,
        text.trim(),
        convId,
        (event: ChatEvent) => {
          if (event.type === "token") {
            setState((s) => ({
              ...s,
              timeline: s.timeline.map((item) =>
                item.id === assistantItem.id
                  ? { ...item, content: (item.content ?? "") + (event.content ?? "") }
                  : item
              ),
            }));
          } else if (event.type === "action_result") {
            const actionName = event.action ?? "";
            const data = event.data ?? {};
            setState((s) => {
              const actionItem: TimelineItem = {
                id: nextId(),
                type: "action_result",
                timestamp: Date.now(),
                action: actionName,
                actionData: data,
                actionSuccess: event.success ?? true,
              };
              const steps = [...s.processingSteps];
              const idx = steps.findIndex((step) => step.action === actionName && !step.done);
              if (idx !== -1) {
                steps[idx] = { ...steps[idx], done: true };
              } else {
                steps.push({ action: actionName, data, done: true });
              }
              return {
                ...s,
                processingSteps: steps,
                timeline: [...s.timeline, actionItem],
              };
            });
          } else if (event.type === "done") {
            const ns = event.nexus_state;
            setState((s) => ({
              ...s,
              conversationId: event.conversation_id ?? s.conversationId,
              streaming: false,
              processingSteps: [],
              timeline: s.timeline.map((item) =>
                item.id === assistantItem.id
                  ? {
                      ...item,
                      streaming: false,
                      phaseNumber: ns?.phase,
                      phaseLabel: ns?.phase_label,
                      milestone: ns?.milestone ?? undefined,
                    }
                  : item
              ),
            }));
            currentAssistantId.current = null;
          }
        },
        abortRef.current.signal,
        provider,
        model,
      );
    } catch (e) {
      const errorMsg = e instanceof Error ? e.message : "Unknown error";
      if (errorMsg !== "AbortError") {
        setState((s) => ({
          ...s,
          streaming: false,
          processingSteps: [],
          timeline: s.timeline.map((item) =>
            item.id === assistantItem.id
              ? { ...item, content: "Something went wrong. Please try again.", streaming: false }
              : item
          ),
        }));
      }
      currentAssistantId.current = null;
    }
  }, [projectId]);

  // --- Run agent task ---
  const runAgent = useCallback(async (task: string, agentProvider?: string, agentModel?: string, agentMode?: string) => {
    if (!task.trim()) return;

    const userItem: TimelineItem = {
      id: nextId(),
      type: "user_message",
      timestamp: Date.now(),
      content: task.trim(),
    };

    setState((s) => ({
      ...s,
      agentRunning: true,
      currentIteration: undefined,
      currentPhase: undefined,
      timeline: [...s.timeline, userItem],
    }));

    const controller = new AbortController();
    abortRef.current = controller;
    try {
      const resp = await fetch(`${BASE}/projects/${projectId}/agent/run`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          task: task.trim(),
          provider: agentProvider || stateRef.current.provider || undefined,
          model: agentModel || stateRef.current.model || undefined,
          mode: agentMode || "auto",
        }),
        signal: controller.signal,
      });

      if (!resp.ok) {
        const errText = await resp.text();
        setState((s) => ({
          ...s,
          agentRunning: false,
          timeline: [...s.timeline, {
            id: nextId(),
            type: "error",
            timestamp: Date.now(),
            errorMessage: errText,
          }],
        }));
        return;
      }

      const reader = resp.body!.getReader();
      const decoder = new TextDecoder();
      let buffer = "";

      while (true) {
        const { done, value } = await reader.read();
        if (done) break;
        buffer += decoder.decode(value, { stream: true });
        const lines = buffer.split("\n");
        buffer = lines.pop() ?? "";

        for (const line of lines) {
          const trimmed = line.trim();
          if (!trimmed || trimmed.startsWith(":")) continue;
          if (trimmed.startsWith("data: ")) {
            try {
              const event = JSON.parse(trimmed.slice(6));
              handleAgentEvent(event);
            } catch { /* ignore */ }
          }
        }
      }
    } catch (e) {
      if ((e as Error).name !== "AbortError") {
        setState((s) => ({
          ...s,
          timeline: [...s.timeline, {
            id: nextId(),
            type: "error",
            timestamp: Date.now(),
            errorMessage: String(e),
          }],
        }));
      }
    }
    setState((s) => ({
      ...s,
      agentRunning: false,
      currentIteration: undefined,
      currentPhase: undefined,
      currentAgent: undefined,
      activeAgents: {},
    }));
  }, [projectId]);

  function handleAgentEvent(event: AgentEvent) {
    const ts = Date.now();
    switch (event.type) {
      // ─── Standard agent loop events ────────────────────────────────
      case "thinking":
        setState((s) => ({
          ...s,
          timeline: [...s.timeline, {
            id: nextId(),
            type: "thinking",
            timestamp: ts,
            content: event.content,
            agentRole: event.agent,
          }],
        }));
        break;

      case "text":
        setState((s) => {
          const last = s.timeline[s.timeline.length - 1];
          if (last?.type === "assistant_message" && last.streaming) {
            return {
              ...s,
              timeline: [
                ...s.timeline.slice(0, -1),
                { ...last, content: (last.content ?? "") + (event.content ?? "") },
              ],
            };
          }
          return {
            ...s,
            timeline: [...s.timeline, {
              id: nextId(),
              type: "assistant_message",
              timestamp: ts,
              content: event.content,
              streaming: true,
            }],
          };
        });
        break;

      case "tool_start":
      case "tool_call":
        setState((s) => {
          const agentRole = event.agent as AgentRoleType | undefined;
          const newAgents = agentRole ? {
            ...s.activeAgents,
            [agentRole]: {
              ...s.activeAgents[agentRole],
              role: agentRole,
              status: "running" as const,
              toolCalls: (s.activeAgents[agentRole]?.toolCalls ?? 0) + 1,
              filesChanged: s.activeAgents[agentRole]?.filesChanged ?? 0,
            },
          } : s.activeAgents;

          return {
            ...s,
            activeAgents: newAgents,
            timeline: [...s.timeline, {
              id: nextId(),
              type: "agent_tool",
              timestamp: ts,
              toolName: event.tool ?? event.name,
              toolArgs: event.arguments,
              agentRole,
            }],
          };
        });
        break;

      case "tool_result":
        setState((s) => {
          const tl = [...s.timeline];
          const toolName = event.tool ?? event.name;
          const idx = [...tl].reverse().findIndex(
            (e) => e.type === "agent_tool" && e.toolName === toolName && e.toolSuccess === undefined
          );
          if (idx >= 0) {
            const realIdx = tl.length - 1 - idx;
            tl[realIdx] = {
              ...tl[realIdx],
              toolOutput: event.output,
              toolSuccess: event.success,
              toolDuration: event.duration_ms,
            };
          }
          return { ...s, timeline: tl };
        });
        break;

      case "iteration":
        setState((s) => {
          const agentRole = event.agent as AgentRoleType | undefined;
          const newAgents = agentRole ? {
            ...s.activeAgents,
            [agentRole]: {
              ...s.activeAgents[agentRole],
              role: agentRole,
              status: "running" as const,
              iteration: { num: event.number ?? 0, max: event.max ?? 30 },
              toolCalls: s.activeAgents[agentRole]?.toolCalls ?? 0,
              filesChanged: s.activeAgents[agentRole]?.filesChanged ?? 0,
            },
          } : s.activeAgents;

          return {
            ...s,
            currentIteration: { num: event.number ?? 0, max: event.max ?? 30 },
            currentAgent: agentRole ?? s.currentAgent,
            activeAgents: newAgents,
          };
        });
        break;

      case "proposal":
        setState((s) => ({
          ...s,
          timeline: [...s.timeline, {
            id: nextId(),
            type: "agent_proposal",
            timestamp: ts,
            proposalIntent: event.intent,
            proposalOperations: event.operations,
            proposalRiskLevel: event.risk_level,
            proposalWarnings: event.warnings,
          }],
        }));
        break;

      // ─── Phase events (from both standard agent loop and coding pipeline) ──
      case "phase":
      case "phase_change":
        setState((s) => {
          const phaseName = event.name ?? event.phase;
          const agentRole = event.agent as AgentRoleType | undefined;
          const status = event.status ?? "running";

          // Update active agents tracking
          const newAgents = { ...s.activeAgents };
          if (agentRole) {
            // Mark previous agent as completed if switching
            if (s.currentAgent && s.currentAgent !== agentRole && newAgents[s.currentAgent]) {
              newAgents[s.currentAgent] = {
                ...newAgents[s.currentAgent],
                status: "completed",
                completedAt: ts,
              };
            }
            newAgents[agentRole] = {
              role: agentRole,
              status: "running",
              toolCalls: newAgents[agentRole]?.toolCalls ?? 0,
              filesChanged: newAgents[agentRole]?.filesChanged ?? 0,
              startedAt: ts,
            };
          }

          return {
            ...s,
            currentPhase: status === "running" || !event.status ? phaseName : undefined,
            currentAgent: agentRole ?? s.currentAgent,
            activeAgents: newAgents,
            timeline: [...s.timeline, {
              id: nextId(),
              type: "agent_phase",
              timestamp: ts,
              phaseName,
              phaseStatus: status,
              agentRole,
            }],
          };
        });
        break;

      // ─── Coding pipeline specific events ───────────────────────────
      case "file_change":
        setState((s) => {
          const change = event.change;
          const agentRole = change?.agent as AgentRoleType | undefined;
          const newAgents = agentRole ? {
            ...s.activeAgents,
            [agentRole]: {
              ...s.activeAgents[agentRole],
              role: agentRole,
              status: "running" as const,
              toolCalls: s.activeAgents[agentRole]?.toolCalls ?? 0,
              filesChanged: (s.activeAgents[agentRole]?.filesChanged ?? 0) + 1,
            },
          } : s.activeAgents;

          return {
            ...s,
            activeAgents: newAgents,
            timeline: [...s.timeline, {
              id: nextId(),
              type: "agent_tool",
              timestamp: ts,
              toolName: change?.change_type === "created" ? "file_write" : "file_edit",
              toolArgs: { path: change?.path },
              toolSuccess: true,
              agentRole,
            }],
          };
        });
        break;

      case "verification":
        setState((s) => ({
          ...s,
          timeline: [...s.timeline, {
            id: nextId(),
            type: "action_result",
            timestamp: ts,
            action: `verify_${event.result?.check_type}`,
            actionSuccess: event.result?.passed,
            actionData: event.result,
          }],
        }));
        break;

      case "error_recovery":
        setState((s) => ({
          ...s,
          timeline: [...s.timeline, {
            id: nextId(),
            type: "milestone",
            timestamp: ts,
            milestone: `${event.strategy} (attempt ${event.retry_count})`,
          }],
        }));
        break;

      case "decision":
        setState((s) => ({
          ...s,
          timeline: [...s.timeline, {
            id: nextId(),
            type: "thinking",
            timestamp: ts,
            content: `[${event.decision?.agent}] ${event.decision?.decision}\n\nReasoning: ${event.decision?.reasoning}`,
            agentRole: event.decision?.agent as AgentRoleType | undefined,
          }],
        }));
        break;

      case "progress":
        setState((s) => ({
          ...s,
          timeline: [...s.timeline, {
            id: nextId(),
            type: "milestone",
            timestamp: ts,
            milestone: `${event.message} (${event.pct}%)`,
            phaseName: event.phase,
          }],
        }));
        break;

      // ─── Completion events ─────────────────────────────────────────
      case "done":
      case "complete":
        setState((s) => {
          // Mark all active agents as completed
          const finalAgents: Record<string, AgentRunStatus> = {};
          for (const [key, agent] of Object.entries(s.activeAgents)) {
            finalAgents[key] = { ...agent, status: "completed", completedAt: ts };
          }

          const summary = event.summary;
          return {
            ...s,
            agentRunning: false,
            currentIteration: undefined,
            currentPhase: undefined,
            currentAgent: undefined,
            activeAgents: finalAgents,
            timeline: [
              ...s.timeline.map((item) =>
                item.streaming ? { ...item, streaming: false } : item
              ),
              {
                id: nextId(),
                type: "agent_done",
                timestamp: ts,
                summary: typeof summary === "string"
                  ? summary
                  : summary?.status
                    ? `Pipeline ${summary.status}. Files: ${(summary.files_created?.length ?? 0) + (summary.files_modified?.length ?? 0)} changed. Iterations: ${summary.total_iterations ?? 0}.`
                    : undefined,
                totalIterations: (typeof summary !== "string" ? summary?.total_iterations : undefined) ?? event.iterations,
              },
            ],
          };
        });
        break;

      case "error":
        setState((s) => ({
          ...s,
          timeline: [...s.timeline, {
            id: nextId(),
            type: "error",
            timestamp: ts,
            errorMessage: event.message,
          }],
          ...(event.fatal ? { agentRunning: false, currentIteration: undefined, currentPhase: undefined } : {}),
        }));
        break;
    }
  }

  // --- Generate plan ---
  const generatePlan = useCallback(async (description: string) => {
    if (!description.trim()) return;

    // Add a thinking indicator while plan generates
    const thinkingItem: TimelineItem = {
      id: nextId(),
      type: "assistant_message",
      timestamp: Date.now(),
      content: "",
      streaming: true,
    };

    setState((s) => ({
      ...s,
      streaming: true,
      timeline: [...s.timeline, thinkingItem],
    }));

    try {
      const res = await api.generatePlan(projectId, description);
      const plan = res.plan as PlanData;
      setState((s) => ({
        ...s,
        streaming: false,
        currentPlan: plan,
        timeline: [
          // Remove the thinking indicator
          ...s.timeline.filter((i) => i.id !== thinkingItem.id),
          {
            id: nextId(),
            type: "plan_block",
            timestamp: Date.now(),
            plan,
            content: plan.summary,
          },
        ],
      }));
    } catch {
      setState((s) => ({
        ...s,
        streaming: false,
        timeline: [
          ...s.timeline.filter((i) => i.id !== thinkingItem.id),
          {
            id: nextId(),
            type: "error",
            timestamp: Date.now(),
            errorMessage: "Failed to generate plan. Try again.",
          },
        ],
      }));
    }
  }, [projectId]);

  // --- Approve plan ---
  const approvePlan = useCallback(async () => {
    if (!stateRef.current.currentPlan) return;
    setState((s) => ({ ...s, streaming: true }));
    try {
      const res = await api.approvePlan(projectId, stateRef.current.currentPlan);
      const result = res.codegen_result as { files_written?: string[]; tables_created?: string[]; agents_configured?: string[] };
      setState((s) => ({
        ...s,
        streaming: false,
        timeline: [...s.timeline, {
          id: nextId(),
          type: "milestone",
          timestamp: Date.now(),
          milestone: `Plan approved and built: ${result.files_written?.length ?? 0} files, ${result.tables_created?.length ?? 0} tables, ${result.agents_configured?.length ?? 0} agents`,
        }],
      }));
    } catch {
      setState((s) => ({
        ...s,
        streaming: false,
        timeline: [...s.timeline, {
          id: nextId(),
          type: "error",
          timestamp: Date.now(),
          errorMessage: "Failed to approve plan",
        }],
      }));
    }
  }, [projectId]);

  // --- Resolve HITL gate ---
  const resolveGate = useCallback(async (gateId: string, approved: boolean, reason?: string) => {
    try {
      await api.resolveGate(projectId, gateId, approved, reason);
      setState((s) => ({
        ...s,
        timeline: s.timeline.map((item) =>
          item.gateId === gateId ? { ...item, gateResolved: true } : item
        ),
      }));
    } catch { /* ignore */ }
  }, [projectId]);

  // --- Stop ---
  const stop = useCallback(() => {
    abortRef.current?.abort();
    abortRef.current = null;
    setState((s) => ({
      ...s,
      streaming: false,
      agentRunning: false,
      processingSteps: [],
      timeline: s.timeline.map((item) =>
        item.streaming ? { ...item, streaming: false } : item
      ),
    }));
  }, []);

  const setMode = useCallback((mode: WorkspaceMode) => {
    setState((s) => ({ ...s, mode }));
  }, []);

  const setModel = useCallback((provider: string, model: string) => {
    setState((s) => ({ ...s, provider, model }));
    api.setProjectModel(projectId, provider, model).catch(() => {});
  }, [projectId]);

  const clearTimeline = useCallback(() => {
    setState((s) => ({
      ...s,
      timeline: [],
      conversationId: undefined,
      currentPlan: undefined,
      processingSteps: [],
      activeAgents: {},
    }));
  }, []);

  // --- Run the coding agent pipeline (multi-agent) ---
  const runCodingPipeline = useCallback(async (
    task: string,
    mode: PipelineMode = "pipeline",
    options?: {
      provider?: string;
      model?: string;
      enableReview?: boolean;
      enableTest?: boolean;
      enableVerification?: boolean;
      enablePerformance?: boolean;
      enableUx?: boolean;
      enableDevops?: boolean;
      enableRefactor?: boolean;
      enableProduct?: boolean;
    },
  ) => {
    if (!task.trim()) return;

    const userItem: TimelineItem = {
      id: nextId(),
      type: "user_message",
      timestamp: Date.now(),
      content: task.trim(),
    };

    setState((s) => ({
      ...s,
      agentRunning: true,
      currentIteration: undefined,
      currentPhase: undefined,
      currentAgent: undefined,
      pipelineMode: mode,
      activeAgents: {},
      timeline: [...s.timeline, userItem],
    }));

    const controller = new AbortController();
    abortRef.current = controller;

    try {
      const resp = await fetch(`${BASE}/projects/${projectId}/coding-agent/run`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          task: task.trim(),
          mode,
          provider: options?.provider || stateRef.current.provider || undefined,
          model: options?.model || stateRef.current.model || undefined,
          enable_review: options?.enableReview,
          enable_test: options?.enableTest,
          enable_verification: options?.enableVerification,
          enable_performance: options?.enablePerformance,
          enable_ux: options?.enableUx,
          enable_devops: options?.enableDevops,
          enable_refactor: options?.enableRefactor,
          enable_product: options?.enableProduct,
        }),
        signal: controller.signal,
      });

      if (!resp.ok) {
        const errText = await resp.text();
        setState((s) => ({
          ...s,
          agentRunning: false,
          activeAgents: {},
          timeline: [...s.timeline, {
            id: nextId(),
            type: "error",
            timestamp: Date.now(),
            errorMessage: errText,
          }],
        }));
        return;
      }

      const reader = resp.body!.getReader();
      const decoder = new TextDecoder();
      let buffer = "";

      while (true) {
        const { done, value } = await reader.read();
        if (done) break;
        buffer += decoder.decode(value, { stream: true });
        const lines = buffer.split("\n");
        buffer = lines.pop() ?? "";

        for (const line of lines) {
          const trimmed = line.trim();
          if (!trimmed || trimmed.startsWith(":")) continue;
          if (trimmed.startsWith("data: ")) {
            try {
              const event = JSON.parse(trimmed.slice(6));
              handleAgentEvent(event);
            } catch { /* ignore parse errors */ }
          }
        }
      }
    } catch (e) {
      if ((e as Error).name !== "AbortError") {
        setState((s) => ({
          ...s,
          timeline: [...s.timeline, {
            id: nextId(),
            type: "error",
            timestamp: Date.now(),
            errorMessage: String(e),
          }],
        }));
      }
    }
    setState((s) => ({
      ...s,
      agentRunning: false,
      currentIteration: undefined,
      currentPhase: undefined,
      currentAgent: undefined,
      activeAgents: Object.fromEntries(
        Object.entries(s.activeAgents).map(([k, v]) => [k, { ...v, status: "completed" as const, completedAt: Date.now() }])
      ),
    }));
  }, [projectId]);

  // --- Wave Pipeline state (50+ agents) ---
  const [wavePipeline, setWavePipeline] = useState<WavePipelineState>({
    running: false,
    totalWaves: 0,
    totalAgents: 0,
    waves: {},
    agents: {},
  });

  // --- Run the 50-agent wave pipeline ---
  const runWavePipeline = useCallback(async (
    task: string,
    options?: {
      provider?: string;
      model?: string;
      waves?: number[];
      skipAgents?: string[];
      onlyAgents?: string[];
    },
  ) => {
    if (!task.trim()) return;

    const userItem: TimelineItem = {
      id: nextId(),
      type: "user_message",
      timestamp: Date.now(),
      content: task.trim(),
    };

    setState((s) => ({
      ...s,
      agentRunning: true,
      timeline: [...s.timeline, userItem],
    }));

    setWavePipeline({
      running: true,
      totalWaves: 0,
      totalAgents: 0,
      waves: {},
      agents: {},
    });

    const controller = new AbortController();
    abortRef.current = controller;

    try {
      const resp = await fetch(`${BASE}/projects/${projectId}/wave-pipeline/run`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          task: task.trim(),
          provider: options?.provider || stateRef.current.provider || undefined,
          model: options?.model || stateRef.current.model || undefined,
          waves: options?.waves,
          skip_agents: options?.skipAgents,
          only_agents: options?.onlyAgents,
        }),
        signal: controller.signal,
      });

      if (!resp.ok) {
        const errText = await resp.text();
        setState((s) => ({
          ...s,
          agentRunning: false,
          timeline: [...s.timeline, { id: nextId(), type: "error", timestamp: Date.now(), errorMessage: errText }],
        }));
        setWavePipeline((p) => ({ ...p, running: false }));
        return;
      }

      const reader = resp.body!.getReader();
      const decoder = new TextDecoder();
      let buffer = "";

      while (true) {
        const { done, value } = await reader.read();
        if (done) break;
        buffer += decoder.decode(value, { stream: true });
        const lines = buffer.split("\n");
        buffer = lines.pop() ?? "";

        for (const line of lines) {
          const trimmed = line.trim();
          if (!trimmed || trimmed.startsWith(":")) continue;
          if (trimmed.startsWith("data: ")) {
            try {
              const event: WaveEvent = JSON.parse(trimmed.slice(6));
              handleWaveEvent(event);
            } catch { /* ignore */ }
          }
        }
      }
    } catch (e) {
      if ((e as Error).name !== "AbortError") {
        setState((s) => ({
          ...s,
          timeline: [...s.timeline, { id: nextId(), type: "error", timestamp: Date.now(), errorMessage: String(e) }],
        }));
      }
    }

    setState((s) => ({ ...s, agentRunning: false }));
    setWavePipeline((p) => ({ ...p, running: false }));
  }, [projectId]);

  function handleWaveEvent(event: WaveEvent) {
    const ts = Date.now();

    switch (event.type) {
      case "wave_pipeline_start":
        setWavePipeline((p) => ({
          ...p,
          running: true,
          totalWaves: event.total_waves ?? 0,
          totalAgents: event.total_agents ?? 0,
        }));
        setState((s) => ({
          ...s,
          timeline: [...s.timeline, {
            id: nextId(), type: "milestone", timestamp: ts,
            milestone: `Wave pipeline started: ${event.total_agents} agents across ${event.total_waves} waves`,
          }],
        }));
        break;

      case "wave_start":
        setWavePipeline((p) => ({
          ...p,
          currentWave: event.wave_index,
          waves: {
            ...p.waves,
            [event.wave_index]: {
              index: event.wave_index,
              label: event.wave_label ?? "",
              status: "running",
              agentCount: event.agent_count ?? 0,
              agentsSucceeded: 0,
              agentsFailed: 0,
            },
          },
        }));
        setState((s) => ({
          ...s,
          timeline: [...s.timeline, {
            id: nextId(), type: "agent_phase", timestamp: ts,
            phaseName: `wave_${event.wave_index}_${event.wave_label}`,
            phaseStatus: "running",
            content: `Wave ${event.wave_index}: ${event.wave_label} (${event.agent_count} agents)`,
          }],
        }));
        break;

      case "wave_agent_start":
        setWavePipeline((p) => ({
          ...p,
          agents: {
            ...p.agents,
            [event.agent_id]: {
              agentId: event.agent_id,
              label: event.agent_label ?? "",
              icon: event.agent_icon ?? "",
              color: event.agent_color ?? "",
              wave: event.wave_index,
              status: "running",
              toolCalls: 0,
              filesChanged: 0,
              startedAt: ts,
            },
          },
        }));
        break;

      case "wave_agent_tool":
        setWavePipeline((p) => ({
          ...p,
          agents: {
            ...p.agents,
            [event.agent_id]: {
              ...p.agents[event.agent_id],
              toolCalls: (p.agents[event.agent_id]?.toolCalls ?? 0) + 1,
            },
          },
        }));
        break;

      case "wave_agent_iteration":
        setWavePipeline((p) => ({
          ...p,
          agents: {
            ...p.agents,
            [event.agent_id]: {
              ...p.agents[event.agent_id],
              iteration: { num: event.number ?? 0, max: event.max ?? 0 },
            },
          },
        }));
        break;

      case "wave_agent_done":
        setWavePipeline((p) => {
          const wave = p.waves[event.wave_index];
          return {
            ...p,
            agents: {
              ...p.agents,
              [event.agent_id]: {
                ...p.agents[event.agent_id],
                status: event.success ? "completed" : "failed",
                summary: typeof event.summary === "string" ? event.summary : undefined,
                durationMs: event.duration_ms,
                filesChanged: event.files_changed ?? 0,
                completedAt: ts,
              },
            },
            waves: wave ? {
              ...p.waves,
              [event.wave_index]: {
                ...wave,
                agentsSucceeded: wave.agentsSucceeded + (event.success ? 1 : 0),
                agentsFailed: wave.agentsFailed + (event.success ? 0 : 1),
              },
            } : p.waves,
          };
        });
        break;

      case "wave_done":
        setWavePipeline((p) => ({
          ...p,
          waves: {
            ...p.waves,
            [event.wave_index]: {
              ...p.waves[event.wave_index],
              status: event.agents_failed > 0 ? "failed" : "completed",
              durationMs: event.duration_ms,
              agentsSucceeded: event.agents_succeeded,
              agentsFailed: event.agents_failed,
            },
          },
        }));
        setState((s) => ({
          ...s,
          timeline: [...s.timeline, {
            id: nextId(), type: "agent_phase", timestamp: ts,
            phaseName: `wave_${event.wave_index}_${event.wave_label}`,
            phaseStatus: "completed",
            content: `Wave ${event.wave_index} done: ${event.agents_succeeded} ok, ${event.agents_failed} failed (${(event.duration_ms / 1000).toFixed(1)}s)`,
          }],
        }));
        break;

      case "wave_agent_thinking":
        setState((s) => ({
          ...s,
          timeline: [...s.timeline, {
            id: nextId(), type: "thinking", timestamp: ts,
            content: `[${event.agent_id}] ${event.content}`,
          }],
        }));
        break;

      case "wave_file_change":
        setWavePipeline((p) => ({
          ...p,
          agents: {
            ...p.agents,
            [event.agent_id]: {
              ...p.agents[event.agent_id],
              filesChanged: (p.agents[event.agent_id]?.filesChanged ?? 0) + 1,
            },
          },
        }));
        break;

      case "wave_pipeline_done": {
        const summary = typeof event.summary !== "string" ? event.summary : undefined;
        setWavePipeline((p) => ({ ...p, running: false }));
        setState((s) => ({
          ...s,
          agentRunning: false,
          timeline: [...s.timeline, {
            id: nextId(), type: "agent_done", timestamp: ts,
            summary: `Pipeline ${summary?.status}: ${summary?.agents_succeeded}/${summary?.total_agents_run} agents succeeded. ${(summary?.files_created?.length ?? 0) + (summary?.files_modified?.length ?? 0)} files changed. ${((summary?.duration_ms ?? 0) / 1000).toFixed(1)}s total.`,
            totalIterations: summary?.total_iterations,
          }],
        }));
        break;
      }

      case "wave_error":
        setState((s) => ({
          ...s,
          timeline: [...s.timeline, {
            id: nextId(), type: "error", timestamp: ts,
            errorMessage: event.message,
          }],
          ...(event.fatal ? { agentRunning: false } : {}),
        }));
        if (event.agent_id) {
          setWavePipeline((p) => ({
            ...p,
            agents: {
              ...p.agents,
              [event.agent_id]: {
                ...p.agents[event.agent_id],
                status: "failed",
              },
            },
          }));
        }
        break;
    }
  }

  return {
    state,
    wavePipeline,
    sendMessage,
    runAgent,
    runCodingPipeline,
    runWavePipeline,
    generatePlan,
    approvePlan,
    resolveGate,
    stop,
    setMode,
    setModel,
    clearTimeline,
  };
}
