"use client";

import { useState } from "react";
import { ChevronDown, ChevronRight, Brain, Scale, XCircle, Lightbulb } from "lucide-react";
import { cn } from "@/lib/utils";

export interface DecisionExplanation {
  area: string;
  choice: string;
  primaryReason: string;
  factors: { name: string; weight: number; description: string }[];
  alternatives: { name: string; reason: string }[];
  learningInfluence: number; // 0-1, how much past learning affected this
}

interface Props {
  explanation: DecisionExplanation | null;
  className?: string;
}

export function ExplanationPanel({ explanation, className }: Props) {
  const [showAlternatives, setShowAlternatives] = useState(false);

  if (!explanation) {
    return (
      <div className={cn("text-center py-8 text-sm text-slate-400", className)}>
        Select a decision to see its explanation
      </div>
    );
  }

  return (
    <div className={cn("space-y-5", className)}>
      {/* Header */}
      <div>
        <p className="text-[11px] text-slate-400 uppercase tracking-wider font-medium">
          Why {explanation.choice}?
        </p>
        <p className="text-sm text-slate-200 mt-1.5 leading-relaxed">
          {explanation.primaryReason}
        </p>
      </div>

      {/* Learning influence */}
      {explanation.learningInfluence > 0 && (
        <div className="flex items-center gap-2 p-2.5 rounded-lg bg-violet-500/[0.06] border border-violet-500/20">
          <Brain className="h-4 w-4 text-violet-400 shrink-0" />
          <div>
            <p className="text-xs font-medium text-violet-400">
              Learning influence: {Math.round(explanation.learningInfluence * 100)}%
            </p>
            <p className="text-[11px] text-slate-400 mt-0.5">
              Past project decisions influenced this choice
            </p>
          </div>
        </div>
      )}

      {/* Contributing factors */}
      <div>
        <p className="text-[11px] text-slate-400 uppercase tracking-wider font-medium mb-2 flex items-center gap-1.5">
          <Scale className="h-3 w-3" />
          Contributing factors
        </p>
        <div className="space-y-2">
          {explanation.factors.map((factor) => (
            <div key={factor.name} className="space-y-1">
              <div className="flex items-center justify-between text-[12px]">
                <span className="text-slate-200 font-medium">{factor.name}</span>
                <span className="text-slate-400 tabular-nums">
                  {Math.round(factor.weight * 100)}%
                </span>
              </div>
              <div className="w-full h-1.5 rounded-full bg-white/[0.06] overflow-hidden">
                <div
                  className="h-full rounded-full bg-glow-cyan transition-all duration-500"
                  style={{ width: `${factor.weight * 100}%` }}
                />
              </div>
              <p className="text-[11px] text-slate-400">{factor.description}</p>
            </div>
          ))}
        </div>
      </div>

      {/* Alternatives */}
      {explanation.alternatives.length > 0 && (
        <div>
          <button
            onClick={() => setShowAlternatives(!showAlternatives)}
            className="flex items-center gap-1.5 text-[11px] text-slate-400 uppercase tracking-wider font-medium hover:text-slate-200 transition-colors"
          >
            {showAlternatives ? (
              <ChevronDown className="h-3 w-3" />
            ) : (
              <ChevronRight className="h-3 w-3" />
            )}
            <XCircle className="h-3 w-3" />
            Alternatives considered ({explanation.alternatives.length})
          </button>

          {showAlternatives && (
            <div className="mt-2 space-y-2">
              {explanation.alternatives.map((alt) => (
                <div
                  key={alt.name}
                  className="flex items-start gap-2 p-2.5 rounded-lg bg-white/[0.02] border border-white/[0.06]"
                >
                  <Lightbulb className="h-3.5 w-3.5 text-slate-400/50 mt-0.5 shrink-0" />
                  <div>
                    <p className="text-[12px] font-medium text-slate-200/70">{alt.name}</p>
                    <p className="text-[11px] text-slate-400 mt-0.5">{alt.reason}</p>
                  </div>
                </div>
              ))}
            </div>
          )}
        </div>
      )}
    </div>
  );
}
