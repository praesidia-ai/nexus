"use client";

import { useQuery } from "@tanstack/react-query";
import { DollarSign } from "lucide-react";
import { cn } from "@/lib/utils";
import { BASE } from "@/lib/api";

interface CostData {
  today_cost_usd: number;
  total_cost_usd: number;
}

async function fetchCostSummary(): Promise<CostData> {
  const res = await fetch(`${BASE}/costs/summary`);
  if (!res.ok) throw new Error("Failed to fetch cost summary");
  return res.json();
}

export function CostIndicator({ className }: { className?: string }) {
  const { data: cost } = useQuery({
    queryKey: ["costs", "summary"],
    queryFn: fetchCostSummary,
    refetchInterval: 30_000,
    retry: false,
  });

  const todayCost = cost?.today_cost_usd ?? 0;
  const totalCost = cost?.total_cost_usd ?? 0;

  return (
    <div
      className={cn(
        "flex items-center gap-1.5 text-xs text-slate-400",
        className,
      )}
    >
      <DollarSign className="w-3 h-3 text-amber-400/70" />
      <span title="Today's cost">${todayCost.toFixed(2)}</span>
      <span className="text-slate-400/30">|</span>
      <span title="Total cost" className="text-slate-400/50">
        ${totalCost.toFixed(2)} total
      </span>
    </div>
  );
}
