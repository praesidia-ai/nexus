"use client";

import { useState } from "react";
import {
  Brain,
  Target,
  GitBranch,
  Code2,
  Sparkles,
  CheckCircle2,
  Loader2,
  Circle,
  ChevronDown,
  ChevronRight,
} from "lucide-react";
import { cn } from "@/lib/utils";
import type { GenerationEvent } from "@/hooks/useGeneration";

interface PhaseInfo {
  id: string;
  label: string;
  icon: React.ElementType;
  color: string;
}

const PHASES: PhaseInfo[] = [
  { id: "learning", label: "Learning", icon: Brain, color: "text-violet-400" },
  { id: "intent", label: "Intent Analysis", icon: Target, color: "text-sky-400" },
  { id: "decisions", label: "Architecture Decisions", icon: GitBranch, color: "text-amber-400" },
  { id: "codegen", label: "Code Generation", icon: Code2, color: "text-emerald-400" },
  { id: "taste", label: "Quality Scoring", icon: Sparkles, color: "text-pink-400" },
  { id: "complete", label: "Complete", icon: CheckCircle2, color: "text-emerald-500" },
];

type PhaseStatus = "pending" | "active" | "complete";

function getPhaseStatus(phaseId: string, currentPhase: string): PhaseStatus {
  const currentIdx = PHASES.findIndex((p) => p.id === currentPhase);
  const phaseIdx = PHASES.findIndex((p) => p.id === phaseId);
  if (phaseIdx < currentIdx) return "complete";
  if (phaseIdx === currentIdx) return "active";
  return "pending";
}

function statusIcon(status: PhaseStatus, Icon: React.ElementType, color: string) {
  switch (status) {
    case "complete":
      return <CheckCircle2 className="h-4 w-4 text-emerald-400" />;
    case "active":
      return <Loader2 className={cn("h-4 w-4 animate-spin", color)} />;
    default:
      return <Circle className="h-4 w-4 text-slate-400/30" />;
  }
}

interface Props {
  currentPhase: string;
  events: GenerationEvent[];
  className?: string;
}

export function PhaseVisualizer({ currentPhase, events, className }: Props) {
  const [expandedPhase, setExpandedPhase] = useState<string | null>(null);

  function phaseEvents(phaseId: string): GenerationEvent[] {
    // Collect events that belong to a phase
    let inPhase = false;
    const result: GenerationEvent[] = [];
    for (const event of events) {
      if (event.data.type === "phase" && event.data.phase === phaseId) {
        inPhase = true;
        continue;
      }
      if (event.data.type === "phase" && event.data.phase !== phaseId) {
        inPhase = false;
        continue;
      }
      if (inPhase) {
        result.push(event);
      }
    }
    return result;
  }

  function phaseDuration(phaseId: string): string | null {
    const pEvents = phaseEvents(phaseId);
    if (pEvents.length < 2) return null;
    const first = pEvents[0].timestamp.getTime();
    const last = pEvents[pEvents.length - 1].timestamp.getTime();
    const ms = last - first;
    if (ms < 1000) return `${ms}ms`;
    return `${(ms / 1000).toFixed(1)}s`;
  }

  return (
    <div className={cn("space-y-1", className)}>
      {PHASES.map((phase, idx) => {
        const status = getPhaseStatus(phase.id, currentPhase);
        const isExpanded = expandedPhase === phase.id;
        const pEvents = phaseEvents(phase.id);
        const duration = phaseDuration(phase.id);
        const hasEvents = pEvents.length > 0;

        return (
          <div key={phase.id}>
            {/* Phase row */}
            <button
              onClick={() => hasEvents && setExpandedPhase(isExpanded ? null : phase.id)}
              className={cn(
                "w-full flex items-center gap-3 px-3 py-2.5 rounded-lg text-left transition-colors",
                status === "active" && "bg-white/[0.05]",
                status === "complete" && "opacity-80",
                hasEvents && "hover:bg-white/[0.04] cursor-pointer",
                !hasEvents && "cursor-default"
              )}
            >
              {/* Connector line */}
              <div className="relative flex flex-col items-center">
                {statusIcon(status, phase.icon, phase.color)}
                {idx < PHASES.length - 1 && (
                  <div
                    className={cn(
                      "absolute top-6 w-px h-4",
                      status === "complete" ? "bg-emerald-500/40" : "bg-white/[0.06]"
                    )}
                  />
                )}
              </div>

              <div className="flex-1 min-w-0">
                <div className="flex items-center gap-2">
                  <phase.icon className={cn("h-3.5 w-3.5 shrink-0", status === "pending" ? "text-slate-400/40" : phase.color)} />
                  <span
                    className={cn(
                      "text-[13px] font-medium",
                      status === "pending" ? "text-slate-400/50" : "text-slate-200"
                    )}
                  >
                    {phase.label}
                  </span>
                </div>
              </div>

              {duration && (
                <span className="text-[11px] text-slate-400 tabular-nums shrink-0">
                  {duration}
                </span>
              )}

              {hasEvents && (
                <span className="shrink-0 text-slate-400/40">
                  {isExpanded ? (
                    <ChevronDown className="h-3.5 w-3.5" />
                  ) : (
                    <ChevronRight className="h-3.5 w-3.5" />
                  )}
                </span>
              )}
            </button>

            {/* Expanded events */}
            {isExpanded && hasEvents && (
              <div className="ml-9 pl-3 border-l border-white/[0.06] mb-2">
                {pEvents.slice(0, 20).map((ev) => (
                  <div
                    key={ev.id}
                    className="py-1 text-[11px] text-slate-400 flex items-center gap-2"
                  >
                    <span className="text-slate-400/40 tabular-nums shrink-0">
                      {ev.timestamp.toLocaleTimeString("en", {
                        hour12: false,
                        hour: "2-digit",
                        minute: "2-digit",
                        second: "2-digit",
                      })}
                    </span>
                    <span className="truncate">
                      {ev.data.message ?? ev.data.detail ?? ev.data.path ?? ev.type}
                    </span>
                  </div>
                ))}
                {pEvents.length > 20 && (
                  <div className="py-1 text-[10px] text-slate-400/40">
                    +{pEvents.length - 20} more events
                  </div>
                )}
              </div>
            )}
          </div>
        );
      })}
    </div>
  );
}
