"use client";

import { useState, useMemo, useCallback, useEffect, useRef } from "react";
import {
  Send,
  Square,
  MessageSquare,
  Sparkles,
  ClipboardList,
  RotateCw,
  PanelLeftClose,
  PanelRightClose,
  PanelLeft,
  PanelRight,
  Command,
} from "lucide-react";
import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { ModelSelector } from "@/components/model-selector";
import { ProjectContextPanel } from "./project-context-panel";
import { UnifiedTimeline } from "./unified-timeline";
import { LiveStatePanel } from "./live-state-panel";
import { useUnifiedWorkspace } from "@/lib/use-unified-workspace";
import type { WorkspaceMode } from "@/lib/unified-types";

interface Props {
  projectId: string;
  projectName: string;
  firstMessage?: string;
}

const MODE_CONFIG: Record<WorkspaceMode, { icon: typeof MessageSquare; label: string; description: string; shortcut: string }> = {
  auto: {
    icon: Sparkles,
    label: "Auto",
    description: "Nexus picks the best approach automatically",
    shortcut: "1",
  },
  chat: {
    icon: MessageSquare,
    label: "Chat",
    description: "Conversational — plan + build via natural language",
    shortcut: "2",
  },
  agent: {
    icon: ClipboardList,
    label: "Agent",
    description: "Autonomous — agent reads, writes, and tests code",
    shortcut: "3",
  },
};

const PLACEHOLDERS: Record<WorkspaceMode, string[]> = {
  chat: [
    "Describe what you want to build...",
    "What should we work on?",
    "Tell me about your project...",
  ],
  agent: [
    "Describe the task for the agent...",
    "What code changes do you need?",
    "What should the agent build?",
  ],
  auto: [
    "Type anything — Nexus figures out the best approach...",
    "What would you like to create?",
    "Describe your idea and Nexus handles the rest...",
  ],
};

export function UnifiedWorkspace({ projectId, projectName, firstMessage }: Props) {
  const {
    state,
    sendMessage,
    runAgent,
    generatePlan,
    approvePlan,
    resolveGate: _resolveGate,
    stop,
    setMode,
    setModel,
    clearTimeline,
  } = useUnifiedWorkspace(projectId);

  const [input, setInput] = useState("");
  const [leftPanelOpen, setLeftPanelOpen] = useState(true);
  const [rightPanelOpen, setRightPanelOpen] = useState(true);
  const [approving, setApproving] = useState(false);
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const firstMessageFired = useRef(false);

  const placeholder = useMemo(
    () => {
      const arr = PLACEHOLDERS[state.mode];
      return arr[Math.floor(Math.random() * arr.length)];
    },
    [state.mode]
  );

  const isActive = state.streaming || state.agentRunning;

  // Fire firstMessage once on mount (correct pattern — useEffect, not useState)
  useEffect(() => {
    if (firstMessage && !firstMessageFired.current && state.timeline.length === 0) {
      firstMessageFired.current = true;
      // Clear URL param
      window.history.replaceState({}, "", `/${projectId}`);
      sendMessage(firstMessage);
    }
  }, [firstMessage, projectId, sendMessage, state.timeline.length]);

  // Keyboard shortcuts
  useEffect(() => {
    function handleGlobalKeyDown(e: KeyboardEvent) {
      // Cmd+K or Ctrl+K: focus input
      if ((e.metaKey || e.ctrlKey) && e.key === "k") {
        e.preventDefault();
        textareaRef.current?.focus();
      }
      // Cmd+1/2/3: switch modes
      if ((e.metaKey || e.ctrlKey) && ["1", "2", "3"].includes(e.key)) {
        e.preventDefault();
        const modes: WorkspaceMode[] = ["auto", "chat", "agent"];
        setMode(modes[parseInt(e.key) - 1]);
      }
      // Cmd+\: toggle left panel
      if ((e.metaKey || e.ctrlKey) && e.key === "\\") {
        e.preventDefault();
        setLeftPanelOpen((prev) => !prev);
      }
      // Cmd+/: toggle right panel
      if ((e.metaKey || e.ctrlKey) && e.key === "/") {
        e.preventDefault();
        setRightPanelOpen((prev) => !prev);
      }
      // Escape: stop streaming
      if (e.key === "Escape" && isActive) {
        stop();
      }
    }
    window.addEventListener("keydown", handleGlobalKeyDown);
    return () => window.removeEventListener("keydown", handleGlobalKeyDown);
  }, [isActive, setMode, stop]);

  // --- Unified send: route based on mode ---
  const handleSend = useCallback(async () => {
    if (!input.trim() || isActive) return;
    const text = input.trim();
    setInput("");
    // Reset textarea height
    if (textareaRef.current) {
      textareaRef.current.style.height = "auto";
    }

    switch (state.mode) {
      case "chat":
        await sendMessage(text);
        break;
      case "agent":
        await runAgent(text);
        break;
      case "auto": {
        const lower = text.toLowerCase();
        const agentKeywords = ["fix", "add ", "implement", "refactor", "write test", "create file", "update ", "delete ", "change ", "build ", "make "];
        const planKeywords = ["plan", "design", "architect"];
        if (agentKeywords.some((k) => lower.includes(k))) {
          await runAgent(text);
        } else if (planKeywords.some((k) => lower.startsWith(k))) {
          await generatePlan(text);
        } else {
          await sendMessage(text);
        }
        break;
      }
    }
  }, [input, isActive, state.mode, sendMessage, runAgent, generatePlan]);

  // Suggestion click
  const handleSuggestionClick = useCallback((text: string) => {
    setInput(text);
    textareaRef.current?.focus();
  }, []);

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      handleSend();
    }
  };

  const handleApprovePlan = useCallback(async () => {
    setApproving(true);
    try {
      await approvePlan();
    } finally {
      setApproving(false);
    }
  }, [approvePlan]);

  const handleRegeneratePlan = useCallback(() => {
    const planIdx = [...state.timeline].reverse().findIndex((i) => i.type === "plan_block");
    if (planIdx >= 0) {
      const realIdx = state.timeline.length - 1 - planIdx;
      for (let i = realIdx - 1; i >= 0; i--) {
        if (state.timeline[i].type === "user_message" && state.timeline[i].content) {
          generatePlan(state.timeline[i].content!);
          return;
        }
      }
    }
  }, [state.timeline, generatePlan]);

  return (
    <div className="flex h-full overflow-hidden">
      {/* Left panel */}
      {leftPanelOpen && (
        <ProjectContextPanel
          projectId={projectId}
          projectName={projectName}
          currentPlan={state.currentPlan}
        />
      )}

      {/* Center column */}
      <div className="flex-1 flex flex-col min-w-0 overflow-hidden">
        {/* Top bar */}
        <div className="shrink-0 flex items-center justify-between px-4 py-2 border-b border-white/[0.06] bg-white/[0.01]">
          <div className="flex items-center gap-2">
            <Button
              variant="ghost"
              size="icon"
              className="h-7 w-7"
              onClick={() => setLeftPanelOpen(!leftPanelOpen)}
              title="Toggle left panel (Cmd+\\)"
              aria-label={leftPanelOpen ? "Hide left panel" : "Show left panel"}
              aria-pressed={leftPanelOpen}
            >
              {leftPanelOpen ? (
                <PanelLeftClose className="w-3.5 h-3.5 text-slate-400" />
              ) : (
                <PanelLeft className="w-3.5 h-3.5 text-slate-400" />
              )}
            </Button>

            {/* Mode selector */}
            <div className="flex items-center rounded-lg border border-white/[0.1] bg-white/[0.03] overflow-hidden">
              {(Object.keys(MODE_CONFIG) as WorkspaceMode[]).map((m) => {
                const conf = MODE_CONFIG[m];
                const Icon = conf.icon;
                return (
                  <button
                    key={m}
                    onClick={() => setMode(m)}
                    disabled={isActive}
                    title={`${conf.label} mode (Cmd+${conf.shortcut})`}
                    className={cn(
                      "px-3 py-1.5 text-[11px] font-medium transition-colors flex items-center gap-1",
                      state.mode === m
                        ? "bg-glow-cyan/[0.12] text-glow-cyan"
                        : "text-slate-400 hover:text-slate-200"
                    )}
                  >
                    <Icon className="w-3 h-3" />
                    {conf.label}
                  </button>
                );
              })}
            </div>

            <span className="text-[10px] text-slate-400 hidden lg:inline">
              {MODE_CONFIG[state.mode].description}
            </span>
          </div>

          <div className="flex items-center gap-2">
            <ModelSelector
              selectedProvider={state.provider}
              selectedModel={state.model}
              onChange={(provider, model) => setModel(provider, model)}
              className="mr-1"
            />

            {state.timeline.length > 0 && !isActive && (
              <Button
                variant="ghost"
                size="sm"
                className="gap-1 text-[11px] h-7 text-slate-400"
                onClick={clearTimeline}
              >
                <RotateCw className="w-3 h-3" />
                New
              </Button>
            )}

            <Button
              variant="ghost"
              size="icon"
              className="h-7 w-7"
              onClick={() => setRightPanelOpen(!rightPanelOpen)}
              title="Toggle right panel (Cmd+/)"
              aria-label={rightPanelOpen ? "Hide right panel" : "Show right panel"}
              aria-pressed={rightPanelOpen}
            >
              {rightPanelOpen ? (
                <PanelRightClose className="w-3.5 h-3.5 text-slate-400" />
              ) : (
                <PanelRight className="w-3.5 h-3.5 text-slate-400" />
              )}
            </Button>
          </div>
        </div>

        {/* Timeline */}
        <UnifiedTimeline
          items={state.timeline}
          streaming={state.streaming}
          agentRunning={state.agentRunning}
          processingSteps={state.processingSteps}
          onApprovePlan={state.currentPlan ? handleApprovePlan : undefined}
          onRegeneratePlan={state.currentPlan ? handleRegeneratePlan : undefined}
          approving={approving}
          onSuggestionClick={handleSuggestionClick}
        />

        {/* Input bar */}
        <div className="shrink-0 border-t border-white/[0.06] glass px-4 py-3">
          <div className="max-w-3xl mx-auto flex gap-3 items-end">
            <div className="flex-1 relative">
              <textarea
                ref={textareaRef}
                value={input}
                onChange={(e) => setInput(e.target.value)}
                onKeyDown={handleKeyDown}
                placeholder={placeholder}
                rows={1}
                disabled={isActive}
                className={cn(
                  "w-full px-4 py-3 pr-20 rounded-xl text-[14px] resize-none",
                  "bg-transparent border border-white/[0.08] text-slate-200",
                  "placeholder:text-slate-400/50",
                  "focus:outline-none focus:shadow-glow-sm focus:border-glow-cyan/40",
                  "min-h-[46px] max-h-[140px] overflow-y-auto",
                  isActive && "opacity-60 cursor-not-allowed"
                )}
                style={{ height: "auto" }}
                onInput={(e) => {
                  const el = e.currentTarget;
                  el.style.height = "auto";
                  el.style.height = Math.min(el.scrollHeight, 140) + "px";
                }}
              />
              {/* Mode badge + keyboard shortcut hint */}
              <div className="absolute right-3 top-1/2 -translate-y-1/2 flex items-center gap-1.5">
                {!isActive && !input && (
                  <span className="text-[9px] text-slate-400/40 hidden sm:flex items-center gap-0.5">
                    <Command className="w-2.5 h-2.5" />K
                  </span>
                )}
                <Badge
                  variant="outline"
                  className={cn(
                    "text-[9px] px-1.5 py-0 pointer-events-none",
                    state.mode === "agent" && "border-violet-500/30 text-violet-400",
                    state.mode === "chat" && "border-glow-cyan/30 text-glow-cyan",
                    state.mode === "auto" && "border-emerald-500/30 text-emerald-400",
                  )}
                >
                  {state.mode}
                </Badge>
              </div>
            </div>
            {isActive ? (
              <Button
                variant="outline"
                size="icon"
                onClick={stop}
                className="flex-shrink-0"
                title="Stop (Escape)"
                aria-label="Stop generation"
              >
                <Square className="w-4 h-4" />
              </Button>
            ) : (
              <Button
                size="icon"
                onClick={handleSend}
                disabled={!input.trim()}
                className="flex-shrink-0"
                aria-label="Send message"
              >
                <Send className="w-4 h-4" />
              </Button>
            )}
          </div>
        </div>
      </div>

      {/* Right panel */}
      {rightPanelOpen && (
        <LiveStatePanel
          projectId={projectId}
          timeline={state.timeline}
          agentRunning={state.agentRunning}
          streaming={state.streaming}
          currentIteration={state.currentIteration}
          currentPhase={state.currentPhase}
          currentAgent={state.currentAgent}
          activeAgents={state.activeAgents}
          pipelineMode={state.pipelineMode}
        />
      )}
    </div>
  );
}
