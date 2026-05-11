"use client";

import { useState, useEffect } from "react";
import { DollarSign, Cpu, TrendingUp, AlertTriangle } from "lucide-react";
import { cn } from "@/lib/utils";
import { api } from "@/lib/api";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";

interface CostSummary {
  total_cost_usd: number;
  by_model: { model: string; cost_usd: number; tokens: number }[];
  today_cost_usd: number;
}

interface CostBudget {
  daily_limit_usd: number;
  used_today_usd: number;
  remaining_usd: number;
  percentage_used: number;
}

interface CostRecommendation {
  type: string;
  description: string;
  savings_usd?: number;
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

interface Props {
  className?: string;
}

export function CostDashboard({ className }: Props) {
  const [summary, setSummary] = useState<CostSummary | null>(null);
  const [budget, setBudget] = useState<CostBudget | null>(null);
  const [recommendations, setRecommendations] = useState<CostRecommendation[]>([]);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    Promise.all([
      api.getCostSummary().catch(() => null),
      api.getCostBudget().catch(() => null),
      api.getCostRecommendations().catch(() => ({ recommendations: [] })),
    ]).then(([sum, bud, recs]) => {
      if (sum) setSummary(sum);
      if (bud) setBudget(bud);
      setRecommendations(recs?.recommendations ?? []);
      setLoading(false);
    });
  }, []);

  if (loading) {
    return (
      <div className={cn("space-y-4 animate-pulse", className)}>
        {[1, 2, 3].map((i) => (
          <div key={i} className="h-24 rounded-xl bg-white/[0.03]" />
        ))}
      </div>
    );
  }

  const budgetPercent = budget?.percentage_used ?? 0;
  const budgetColor =
    budgetPercent >= 90 ? "text-red-400" : budgetPercent >= 70 ? "text-amber-400" : "text-emerald-400";
  const budgetBarColor =
    budgetPercent >= 90 ? "bg-red-400" : budgetPercent >= 70 ? "bg-amber-400" : "bg-emerald-400";

  return (
    <div className={cn("space-y-6", className)}>
      {/* Top KPIs */}
      <div className="grid grid-cols-3 gap-4">
        <Card>
          <CardContent className="p-4 pt-4">
            <div className="flex items-center gap-3">
              <div className="w-9 h-9 rounded-lg bg-emerald-500/10 flex items-center justify-center shrink-0">
                <DollarSign className="h-4.5 w-4.5 text-emerald-400" />
              </div>
              <div>
                <p className="text-[11px] text-slate-400">Total spend</p>
                <p className="text-xl font-bold tabular-nums">
                  {summary ? formatUsd(summary.total_cost_usd) : "--"}
                </p>
              </div>
            </div>
          </CardContent>
        </Card>

        <Card>
          <CardContent className="p-4 pt-4">
            <div className="flex items-center gap-3">
              <div className="w-9 h-9 rounded-lg bg-sky-500/10 flex items-center justify-center shrink-0">
                <TrendingUp className="h-4.5 w-4.5 text-sky-400" />
              </div>
              <div>
                <p className="text-[11px] text-slate-400">Today</p>
                <p className="text-xl font-bold tabular-nums">
                  {summary ? formatUsd(summary.today_cost_usd) : "--"}
                </p>
              </div>
            </div>
          </CardContent>
        </Card>

        <Card>
          <CardContent className="p-4 pt-4">
            <div className="flex items-center gap-3">
              <div className={cn("w-9 h-9 rounded-lg flex items-center justify-center shrink-0", budgetPercent >= 70 ? "bg-amber-500/10" : "bg-emerald-500/10")}>
                <Cpu className={cn("h-4.5 w-4.5", budgetColor)} />
              </div>
              <div>
                <p className="text-[11px] text-slate-400">Budget used</p>
                <p className={cn("text-xl font-bold tabular-nums", budgetColor)}>
                  {budget ? `${Math.round(budgetPercent)}%` : "--"}
                </p>
              </div>
            </div>
          </CardContent>
        </Card>
      </div>

      {/* Budget bar */}
      {budget && (
        <Card>
          <CardContent className="p-4 pt-4">
            <div className="flex items-center justify-between mb-2 text-xs text-slate-400">
              <span>Daily budget: {formatUsd(budget.daily_limit_usd)}</span>
              <span>Remaining: {formatUsd(budget.remaining_usd)}</span>
            </div>
            <div className="w-full h-2 rounded-full bg-white/[0.06] overflow-hidden">
              <div
                className={cn("h-full rounded-full transition-all duration-500", budgetBarColor)}
                style={{ width: `${Math.min(budgetPercent, 100)}%` }}
              />
            </div>
          </CardContent>
        </Card>
      )}

      {/* By-model breakdown */}
      {summary && summary.by_model.length > 0 && (
        <Card>
          <CardHeader className="border-b border-white/[0.06]">
            <CardTitle className="text-sm">Cost by model</CardTitle>
          </CardHeader>
          <CardContent className="p-0">
            <table className="w-full">
              <thead>
                <tr className="border-b border-white/[0.06]">
                  <th className="text-left px-4 py-2.5 text-[11px] font-medium text-slate-400">Model</th>
                  <th className="text-right px-4 py-2.5 text-[11px] font-medium text-slate-400">Tokens</th>
                  <th className="text-right px-4 py-2.5 text-[11px] font-medium text-slate-400">Cost</th>
                  <th className="text-right px-4 py-2.5 text-[11px] font-medium text-slate-400">Share</th>
                </tr>
              </thead>
              <tbody>
                {summary.by_model.map((m) => (
                  <tr key={m.model} className="border-b border-white/[0.04] last:border-0 hover:bg-white/[0.02]">
                    <td className="px-4 py-2 text-[12px] font-medium text-slate-200">{m.model}</td>
                    <td className="px-4 py-2 text-[12px] text-slate-400 text-right tabular-nums">
                      {formatTokens(m.tokens)}
                    </td>
                    <td className="px-4 py-2 text-[12px] text-slate-200 text-right font-medium tabular-nums">
                      {formatUsd(m.cost_usd)}
                    </td>
                    <td className="px-4 py-2 text-[12px] text-slate-400 text-right tabular-nums">
                      {summary.total_cost_usd > 0
                        ? `${Math.round((m.cost_usd / summary.total_cost_usd) * 100)}%`
                        : "0%"}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </CardContent>
        </Card>
      )}

      {/* Recommendations */}
      {recommendations.length > 0 && (
        <Card>
          <CardHeader className="border-b border-white/[0.06]">
            <CardTitle className="text-sm flex items-center gap-2">
              <AlertTriangle className="h-4 w-4 text-amber-400" />
              Cost optimization suggestions
            </CardTitle>
          </CardHeader>
          <CardContent className="pt-4">
            <div className="space-y-2.5">
              {recommendations.map((rec, i) => (
                <div
                  key={i}
                  className="flex items-start gap-2.5 p-2.5 rounded-lg bg-white/[0.02] border border-white/[0.06]"
                >
                  <Badge variant="outline" className="text-[9px] shrink-0 mt-0.5 capitalize">
                    {rec.type}
                  </Badge>
                  <div className="flex-1 min-w-0">
                    <p className="text-[12px] text-slate-200">{rec.description}</p>
                    {rec.savings_usd !== undefined && (
                      <p className="text-[11px] text-emerald-400 mt-0.5">
                        Potential savings: {formatUsd(rec.savings_usd)}
                      </p>
                    )}
                  </div>
                </div>
              ))}
            </div>
          </CardContent>
        </Card>
      )}
    </div>
  );
}
