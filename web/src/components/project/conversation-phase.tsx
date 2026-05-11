"use client";

import { cn } from "@/lib/utils";

/* ------------------------------------------------------------------ */
/*  Phase definitions                                                  */
/* ------------------------------------------------------------------ */

const PHASES = [
  { key: "discovery", label: "Discovery", index: 0 },
  { key: "proposal", label: "Proposal", index: 1 },
  { key: "building", label: "Building", index: 2 },
  { key: "refinement", label: "Refinement", index: 3 },
  { key: "team_creation", label: "Team Creation", index: 4 },
  { key: "operating", label: "Operating", index: 5 },
] as const;

type PhaseKey = (typeof PHASES)[number]["key"];

/* ------------------------------------------------------------------ */
/*  Component                                                          */
/* ------------------------------------------------------------------ */

interface ConversationPhaseProps {
  /** The currently active phase key or numeric index (0-5) */
  activePhase: PhaseKey | number;
  /** Discovery confidence (0-100), shown only during discovery phase */
  discoveryConfidence?: number;
  className?: string;
}

export function ConversationPhase({
  activePhase,
  discoveryConfidence,
  className,
}: ConversationPhaseProps) {
  const activeIndex =
    typeof activePhase === "number"
      ? activePhase
      : PHASES.find((p) => p.key === activePhase)?.index ?? 0;

  return (
    <div className={cn("flex items-center justify-center gap-0", className)}>
      {PHASES.map((phase, i) => {
        const isPast = i < activeIndex;
        const isActive = i === activeIndex;
        const isFuture = i > activeIndex;
        const showConfidence =
          isActive && phase.key === "discovery" && discoveryConfidence != null;

        return (
          <div key={phase.key} className="flex items-center">
            {/* Connector line (before dot, skip for first) */}
            {i > 0 && (
              <div
                className={cn(
                  "h-px w-6 sm:w-10 transition-colors",
                  isPast || isActive ? "bg-glow-cyan/40" : "bg-white/[0.06]"
                )}
              />
            )}

            {/* Phase dot + label */}
            <div className="flex flex-col items-center gap-1">
              <div
                className={cn(
                  "relative h-3 w-3 rounded-full border-2 transition-all",
                  isActive &&
                    "border-glow-cyan bg-glow-cyan shadow-[0_0_8px_rgba(0,200,255,0.5)]",
                  isPast && "border-glow-cyan/60 bg-glow-cyan/60",
                  isFuture && "border-white/[0.12] bg-transparent"
                )}
              >
                {/* Inner glow pulse for active */}
                {isActive && (
                  <span className="absolute inset-0 rounded-full animate-ping bg-glow-cyan/30" />
                )}
              </div>
              <span
                className={cn(
                  "text-[10px] leading-none whitespace-nowrap select-none transition-colors",
                  isActive && "text-glow-cyan font-medium",
                  isPast && "text-slate-400",
                  isFuture && "text-slate-600"
                )}
              >
                {phase.label}
              </span>
              {/* Discovery confidence */}
              {showConfidence && (
                <span className="text-[9px] text-glow-cyan/70 -mt-0.5">
                  {discoveryConfidence}%
                </span>
              )}
            </div>
          </div>
        );
      })}
    </div>
  );
}
