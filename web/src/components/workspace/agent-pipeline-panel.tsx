"use client";

import {
  Compass,
  Code2,
  Shield,
  TestTube2,
  Bug,
  Gauge,
  Palette,
  Container,
  Recycle,
  Lightbulb,
  Loader2,
  CheckCircle2,
  XCircle,
  Circle,
} from "lucide-react";
import { cn } from "@/lib/utils";
import type { AgentRoleType, AgentRunStatus } from "@/lib/unified-types";

// ---------------------------------------------------------------------------
// Agent visual config
// ---------------------------------------------------------------------------

const AGENT_CONFIG: Record<
  AgentRoleType,
  { icon: typeof Compass; color: string; bg: string; border: string; label: string; group: "core" | "quality" | "enhance" }
> = {
  architect:    { icon: Compass,    color: "text-violet-400",  bg: "bg-violet-500/10",  border: "border-violet-500/30",  label: "Architect",    group: "core" },
  coder:        { icon: Code2,      color: "text-blue-400",    bg: "bg-blue-500/10",    border: "border-blue-500/30",    label: "Coder",        group: "core" },
  reviewer:     { icon: Shield,     color: "text-amber-400",   bg: "bg-amber-500/10",   border: "border-amber-500/30",   label: "Reviewer",     group: "quality" },
  tester:       { icon: TestTube2,  color: "text-green-400",   bg: "bg-green-500/10",   border: "border-green-500/30",   label: "Tester",       group: "core" },
  debugger:     { icon: Bug,        color: "text-red-400",     bg: "bg-red-500/10",     border: "border-red-500/30",     label: "Debugger",     group: "core" },
  performance:  { icon: Gauge,      color: "text-orange-400",  bg: "bg-orange-500/10",  border: "border-orange-500/30",  label: "Performance",  group: "quality" },
  ux:           { icon: Palette,    color: "text-pink-400",    bg: "bg-pink-500/10",    border: "border-pink-500/30",    label: "UX",           group: "enhance" },
  devops:       { icon: Container,  color: "text-cyan-400",    bg: "bg-cyan-500/10",    border: "border-cyan-500/30",    label: "DevOps",       group: "enhance" },
  refactor:     { icon: Recycle,    color: "text-teal-400",    bg: "bg-teal-500/10",    border: "border-teal-500/30",    label: "Refactor",     group: "quality" },
  product:      { icon: Lightbulb,  color: "text-yellow-400",  bg: "bg-yellow-500/10",  border: "border-yellow-500/30",  label: "Product",      group: "enhance" },
};

// ---------------------------------------------------------------------------
// Pipeline visualization
// ---------------------------------------------------------------------------

const PIPELINE_STAGES: { label: string; agents: AgentRoleType[] }[] = [
  { label: "Plan", agents: ["architect"] },
  { label: "Implement", agents: ["coder"] },
  { label: "Quality Gate", agents: ["reviewer", "performance", "refactor"] },
  { label: "Test", agents: ["tester"] },
  { label: "Verify & Debug", agents: ["debugger"] },
  { label: "Enhance", agents: ["ux", "product", "devops"] },
];

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

interface AgentPipelinePanelProps {
  activeAgents: Record<string, AgentRunStatus>;
  currentAgent?: AgentRoleType;
  currentPhase?: string;
  pipelineMode?: string;
  isRunning: boolean;
}

export function AgentPipelinePanel({
  activeAgents,
  currentAgent,
  currentPhase: _currentPhase,
  pipelineMode,
  isRunning,
}: AgentPipelinePanelProps) {
  const isFullPipeline = pipelineMode === "full_pipeline" || pipelineMode === "full";

  // Determine which stages to show based on pipeline mode
  const visibleStages = isFullPipeline
    ? PIPELINE_STAGES
    : PIPELINE_STAGES.filter((s) => s.label !== "Enhance" && s.agents.length <= 1 || s.label === "Quality Gate" && !isFullPipeline
        ? s.agents.filter((a) => a === "reviewer").length > 0
        : true
      ).map((s) => ({
        ...s,
        agents: isFullPipeline ? s.agents : s.agents.filter((a) => ["architect", "coder", "reviewer", "tester", "debugger"].includes(a)),
      })).filter((s) => s.agents.length > 0);

  return (
    <div className="space-y-3">
      {/* Pipeline header */}
      <div className="flex items-center justify-between">
        <span className="text-[11px] font-semibold uppercase tracking-wider text-white/40">
          Agent Pipeline
        </span>
        {isRunning && (
          <span className="flex items-center gap-1.5 text-[10px] text-emerald-400">
            <span className="w-1.5 h-1.5 rounded-full bg-emerald-400 animate-pulse" />
            Running
          </span>
        )}
      </div>

      {/* Pipeline stages */}
      <div className="space-y-1">
        {visibleStages.map((stage, i) => (
          <div key={stage.label}>
            {/* Stage label */}
            <div className="flex items-center gap-2 mb-1">
              <span className="text-[10px] font-medium text-white/30 uppercase tracking-wide">
                {stage.label}
              </span>
              {stage.agents.length > 1 && (
                <span className="text-[9px] text-white/20 italic">parallel</span>
              )}
            </div>

            {/* Agent cards in stage */}
            <div className={cn(
              "grid gap-1.5",
              stage.agents.length > 1 ? "grid-cols-3" : "grid-cols-1"
            )}>
              {stage.agents.map((role) => {
                const config = AGENT_CONFIG[role];
                const status = activeAgents[role];
                const isActive = currentAgent === role;
                const Icon = config.icon;

                return (
                  <div
                    key={role}
                    className={cn(
                      "flex items-center gap-2 px-2.5 py-1.5 rounded-lg border transition-all duration-300",
                      status?.status === "running"
                        ? `${config.border} ${config.bg}`
                        : status?.status === "completed"
                          ? "border-emerald-500/20 bg-emerald-500/5"
                          : status?.status === "failed"
                            ? "border-red-500/20 bg-red-500/5"
                            : "border-white/[0.06] bg-white/[0.02]",
                      isActive && "ring-1 ring-white/10"
                    )}
                  >
                    {/* Status indicator */}
                    {status?.status === "running" ? (
                      <Loader2 className={cn("w-3 h-3 animate-spin shrink-0", config.color)} />
                    ) : status?.status === "completed" ? (
                      <CheckCircle2 className="w-3 h-3 text-emerald-400 shrink-0" />
                    ) : status?.status === "failed" ? (
                      <XCircle className="w-3 h-3 text-red-400 shrink-0" />
                    ) : (
                      <Circle className="w-3 h-3 text-white/15 shrink-0" />
                    )}

                    <Icon className={cn(
                      "w-3 h-3 shrink-0",
                      status?.status === "running" ? config.color
                        : status?.status === "completed" ? "text-emerald-400"
                        : status?.status === "failed" ? "text-red-400"
                        : "text-white/20"
                    )} />

                    <span className={cn(
                      "text-[11px] font-medium truncate",
                      status?.status === "running" ? config.color
                        : status?.status === "completed" ? "text-emerald-400/80"
                        : status?.status === "failed" ? "text-red-400/80"
                        : "text-white/25"
                    )}>
                      {config.label}
                    </span>

                    {/* Iteration counter for running agent */}
                    {status?.status === "running" && status.iteration && (
                      <span className="ml-auto text-[9px] text-white/30">
                        {status.iteration.num}/{status.iteration.max}
                      </span>
                    )}

                    {/* Tool count for completed agent */}
                    {status?.status === "completed" && status.toolCalls > 0 && (
                      <span className="ml-auto text-[9px] text-white/20">
                        {status.toolCalls}t
                      </span>
                    )}
                  </div>
                );
              })}
            </div>

            {/* Connector arrow between stages */}
            {i < visibleStages.length - 1 && (
              <div className="flex justify-center py-0.5">
                <div className="w-px h-2 bg-white/10" />
              </div>
            )}
          </div>
        ))}
      </div>

      {/* Summary stats */}
      {Object.keys(activeAgents).length > 0 && (
        <div className="flex items-center gap-3 pt-2 border-t border-white/[0.06]">
          <div className="text-[10px] text-white/30">
            <span className="text-white/50 font-medium">
              {Object.values(activeAgents).filter((a) => a.status === "completed").length}
            </span>
            /{Object.keys(activeAgents).length} agents done
          </div>
          <div className="text-[10px] text-white/30">
            <span className="text-white/50 font-medium">
              {Object.values(activeAgents).reduce((sum, a) => sum + (a.toolCalls ?? 0), 0)}
            </span>
            {" "}tool calls
          </div>
          <div className="text-[10px] text-white/30">
            <span className="text-white/50 font-medium">
              {Object.values(activeAgents).reduce((sum, a) => sum + (a.filesChanged ?? 0), 0)}
            </span>
            {" "}files
          </div>
        </div>
      )}
    </div>
  );
}
