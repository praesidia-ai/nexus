"use client";

import { useState, useCallback, useRef, useEffect } from "react";
import { useSSE } from "./useSSE";
import { BASE } from "@/lib/api";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export interface MemberStatus {
  id: string;
  name: string;
  role: string;
  model: string;
  status: "idle" | "working" | "done" | "failed" | "waiting";
  currentTask?: string;
  progress: number; // 0-100
  tokensUsed: number;
  cost: number;
  avatar?: string;
}

export interface TeamMessage {
  id: string;
  timestamp: string;
  from: string;
  to: string | "all";
  type: "task" | "review" | "proposal" | "vote" | "escalation" | "info" | "result";
  content: string;
  metadata?: Record<string, unknown>;
}

export interface TeamTask {
  id: string;
  title: string;
  description: string;
  assignee?: string;
  status: "backlog" | "ready" | "in_progress" | "in_review" | "done" | "failed";
  priority: "low" | "medium" | "high" | "critical";
  createdAt: string;
  updatedAt: string;
  output?: string;
}

export interface TeamArtifact {
  id: string;
  name: string;
  type: string;
  content: string;
  createdBy: string;
  createdAt: string;
  size: number;
}

export interface TeamEvent {
  id: string;
  timestamp: string;
  type: string;
  summary: string;
  data?: Record<string, unknown>;
}

export interface TeamExecutionState {
  teamId: string;
  task: string;
  status: "idle" | "running" | "paused" | "completed" | "failed";
  members: MemberStatus[];
  messages: TeamMessage[];
  tasks: TeamTask[];
  artifacts: TeamArtifact[];
  budget: { spent: number; remaining: number; percentUsed: number };
  duration: number;
  events: TeamEvent[];
}

// ---------------------------------------------------------------------------
// SSE event shape from backend
// ---------------------------------------------------------------------------

interface TeamSSEEvent {
  type:
    | "status_change"
    | "member_update"
    | "message"
    | "task_update"
    | "artifact"
    | "budget_update"
    | "event"
    | "completed"
    | "failed"
    | "heartbeat";
  data: Record<string, unknown>;
}

// ---------------------------------------------------------------------------
// Initial state factory
// ---------------------------------------------------------------------------

function makeInitialState(teamId: string): TeamExecutionState {
  return {
    teamId,
    task: "",
    status: "idle",
    members: [],
    messages: [],
    tasks: [],
    artifacts: [],
    budget: { spent: 0, remaining: 0, percentUsed: 0 },
    duration: 0,
    events: [],
  };
}

// ---------------------------------------------------------------------------
// Hook
// ---------------------------------------------------------------------------

export function useTeamExecution(runId: string | null) {
  const [state, setState] = useState<TeamExecutionState>(
    makeInitialState(runId ?? "")
  );

  const timerRef = useRef<ReturnType<typeof setInterval> | null>(null);
  const startTimeRef = useRef<number | null>(null);

  // Duration timer
  useEffect(() => {
    if (state.status === "running") {
      if (!startTimeRef.current) startTimeRef.current = Date.now();
      timerRef.current = setInterval(() => {
        setState((prev) => ({
          ...prev,
          duration: Math.floor((Date.now() - (startTimeRef.current ?? Date.now())) / 1000),
        }));
      }, 1000);
    } else {
      if (timerRef.current) {
        clearInterval(timerRef.current);
        timerRef.current = null;
      }
    }
    return () => {
      if (timerRef.current) clearInterval(timerRef.current);
    };
  }, [state.status]);

  // SSE connection
  const sseUrl = runId ? `${BASE}/teams/runs/${runId}/stream` : null;

  useSSE<TeamSSEEvent>(
    sseUrl,
    useCallback((event: TeamSSEEvent) => {
      setState((prev) => {
        switch (event.type) {
          case "status_change":
            return {
              ...prev,
              status: (event.data.status as TeamExecutionState["status"]) ?? prev.status,
              task: (event.data.task as string) ?? prev.task,
            };

          case "member_update": {
            const member = event.data as unknown as MemberStatus;
            const idx = prev.members.findIndex((m) => m.id === member.id);
            const members = [...prev.members];
            if (idx >= 0) {
              members[idx] = { ...members[idx], ...member };
            } else {
              members.push(member);
            }
            return { ...prev, members };
          }

          case "message": {
            const msg = event.data as unknown as TeamMessage;
            return {
              ...prev,
              messages: [...prev.messages, { ...msg, id: msg.id || crypto.randomUUID() }],
            };
          }

          case "task_update": {
            const task = event.data as unknown as TeamTask;
            const tidx = prev.tasks.findIndex((t) => t.id === task.id);
            const tasks = [...prev.tasks];
            if (tidx >= 0) {
              tasks[tidx] = { ...tasks[tidx], ...task };
            } else {
              tasks.push(task);
            }
            return { ...prev, tasks };
          }

          case "artifact": {
            const artifact = event.data as unknown as TeamArtifact;
            return {
              ...prev,
              artifacts: [...prev.artifacts, artifact],
            };
          }

          case "budget_update":
            return {
              ...prev,
              budget: {
                spent: (event.data.spent as number) ?? prev.budget.spent,
                remaining: (event.data.remaining as number) ?? prev.budget.remaining,
                percentUsed: (event.data.percentUsed as number) ?? prev.budget.percentUsed,
              },
            };

          case "event": {
            const evt = event.data as unknown as TeamEvent;
            return {
              ...prev,
              events: [...prev.events, { ...evt, id: evt.id || crypto.randomUUID() }],
            };
          }

          case "completed":
            return { ...prev, status: "completed" };

          case "failed":
            return { ...prev, status: "failed" };

          default:
            return prev;
        }
      });
    }, [])
  );

  // Fetch initial state when runId changes
  useEffect(() => {
    if (!runId) {
      setState(makeInitialState(""));
      return;
    }

    setState(makeInitialState(runId));
    startTimeRef.current = null;

    fetch(`${BASE}/teams/runs/${runId}`)
      .then((res) => {
        if (!res.ok) return null;
        return res.json();
      })
      .then((data) => {
        if (data) {
          setState((prev) => ({
            ...prev,
            teamId: data.team_id ?? prev.teamId,
            task: data.task ?? prev.task,
            status: data.status ?? prev.status,
            members: data.members ?? prev.members,
            messages: data.messages ?? prev.messages,
            tasks: data.tasks ?? prev.tasks,
            artifacts: data.artifacts ?? prev.artifacts,
            budget: data.budget ?? prev.budget,
            duration: data.duration ?? prev.duration,
          }));
          if (data.status === "running") {
            startTimeRef.current = Date.now() - (data.duration ?? 0) * 1000;
          }
        }
      })
      .catch(() => {});
  }, [runId]);

  // Actions
  const inject = useCallback(
    async (message: string, target?: string) => {
      if (!runId) return;
      await fetch(`${BASE}/teams/runs/${runId}/inject`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ message, target }),
      });
    },
    [runId]
  );

  const pause = useCallback(async (): Promise<{ ok: boolean; error?: string }> => {
    if (!runId) return { ok: false, error: "No active run" };
    try {
      const res = await fetch(`${BASE}/teams/runs/${runId}/pause`, { method: "POST" });
      if (!res.ok) {
        if (res.status === 404) {
          return { ok: false, error: "Run not found — it may have already finished" };
        }
        return { ok: false, error: `HTTP ${res.status}` };
      }
      return { ok: true };
    } catch (err) {
      return { ok: false, error: err instanceof Error ? err.message : String(err) };
    }
  }, [runId]);

  const resume = useCallback(async (): Promise<{ ok: boolean; error?: string }> => {
    if (!runId) return { ok: false, error: "No active run" };
    try {
      const res = await fetch(`${BASE}/teams/runs/${runId}/resume`, { method: "POST" });
      if (!res.ok) {
        if (res.status === 404) {
          return { ok: false, error: "Run not found — it may have already finished" };
        }
        return { ok: false, error: `HTTP ${res.status}` };
      }
      return { ok: true };
    } catch (err) {
      return { ok: false, error: err instanceof Error ? err.message : String(err) };
    }
  }, [runId]);

  return { state, inject, pause, resume };
}
