"use client";

import { DollarSign, Cpu, ArrowUpRight, ArrowDownRight } from "lucide-react";
import { cn } from "@/lib/utils";

export interface CostData {
  totalUsd: number;
  inputTokens: number;
  outputTokens: number;
  model: string;
  phaseBreakdown?: { phase: string; costUsd: number; tokens: number }[];
}

interface Props {
  cost: CostData | null;
  className?: string;
}

function formatUsd(amount: number): string {
  if (amount < 0.01) return `$${amount.toFixed(4)}`;
  if (amount < 1) return `$${amount.toFixed(3)}`;
  return `$${amount.toFixed(2)}`;
}

function formatTokens(count: number): string {
  if (count >= 1_000_000) return `${(count / 1_000_000).toFixed(1)}M`;
  if (count >= 1_000) return `${(count / 1_000).toFixed(1)}K`;
  return String(count);
}

export function CostTracker({ cost, className }: Props) {
  if (!cost) {
    return (
      <div className={cn("text-center py-4 text-sm text-slate-400", className)}>
        Cost data will appear during generation
      </div>
    );
  }

  return (
    <div className={cn("space-y-4", className)}>
      {/* Total cost */}
      <div className="flex items-center gap-3">
        <div className="w-9 h-9 rounded-lg bg-emerald-500/10 flex items-center justify-center shrink-0">
          <DollarSign className="h-4 w-4 text-emerald-400" />
        </div>
        <div>
          <p className="text-xl font-bold tabular-nums text-slate-200">{formatUsd(cost.totalUsd)}</p>
          <p className="text-[11px] text-slate-400">Total cost</p>
        </div>
      </div>

      {/* Token counts */}
      <div className="grid grid-cols-2 gap-3">
        <div className="flex items-center gap-2 p-2.5 rounded-lg bg-white/[0.03] border border-white/[0.06]">
          <ArrowDownRight className="h-3.5 w-3.5 text-sky-400 shrink-0" />
          <div>
            <p className="text-sm font-semibold tabular-nums">{formatTokens(cost.inputTokens)}</p>
            <p className="text-[10px] text-slate-400">Input tokens</p>
          </div>
        </div>
        <div className="flex items-center gap-2 p-2.5 rounded-lg bg-white/[0.03] border border-white/[0.06]">
          <ArrowUpRight className="h-3.5 w-3.5 text-amber-400 shrink-0" />
          <div>
            <p className="text-sm font-semibold tabular-nums">{formatTokens(cost.outputTokens)}</p>
            <p className="text-[10px] text-slate-400">Output tokens</p>
          </div>
        </div>
      </div>

      {/* Model */}
      <div className="flex items-center gap-2 text-xs text-slate-400">
        <Cpu className="h-3 w-3 shrink-0" />
        <span>Model: <span className="text-slate-200 font-medium">{cost.model}</span></span>
      </div>

      {/* Phase breakdown */}
      {cost.phaseBreakdown && cost.phaseBreakdown.length > 0 && (
        <div className="space-y-1.5">
          <p className="text-[11px] text-slate-400 font-medium uppercase tracking-wider">
            Per-phase cost
          </p>
          {cost.phaseBreakdown.map((phase) => (
            <div
              key={phase.phase}
              className="flex items-center justify-between text-[12px] py-1 border-b border-white/[0.04] last:border-0"
            >
              <span className="text-slate-400 capitalize">{phase.phase}</span>
              <div className="flex items-center gap-3">
                <span className="text-slate-400/60 tabular-nums text-[10px]">
                  {formatTokens(phase.tokens)} tok
                </span>
                <span className="text-slate-200 font-medium tabular-nums">
                  {formatUsd(phase.costUsd)}
                </span>
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
