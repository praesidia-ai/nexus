"use client";

import { useState, useEffect, useCallback } from "react";
import { useParams } from "next/navigation";
import {
  Activity,
  BarChart3,
  Brain,
  CheckCircle2,
  ChevronDown,
  ChevronRight,
  Clock,
  Cpu,
  DollarSign,
  Heart,
  RefreshCw,
  Server,
  Sparkles,
  TrendingUp,
  XCircle,
  Zap,
} from "lucide-react";
import { cn } from "@/lib/utils";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { Skeleton } from "@/components/ui/skeleton";
import { PageHeader } from "@/components/page-header";
import { StaggerItem } from "@/components/ui/stagger-list";
import { BASE } from "@/lib/api";
import { useVisibleInterval } from "@/hooks/use-visible-interval";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface CostSummary {
  date: string;
  total_calls: number;
  total_cost_usd: number;
  total_tokens: number;
  records: CostRecord[];
}

interface CostRecord {
  model?: string;
  provider?: string;
  cost_usd?: number;
  total_tokens?: number;
  prompt_tokens?: number;
  completion_tokens?: number;
  operation?: string;
}

interface Trace {
  id: string;
  project_id: string;
  agent_name?: string;
  action?: string;
  status?: string;
  duration_ms?: number;
  cost_usd?: number;
  tokens_used?: number;
  created_at: string;
  detail?: string;
}

interface RunMetric {
  id: string;
  project_id: string;
  run_id: string;
  status: string;
  duration_ms?: number;
  tokens_used?: number;
  cost_usd?: number;
  created_at: string;
}

interface MemoryHealth {
  episode_count: number;
  fact_count: number;
  last_consolidation?: string;
  /** Backend doesn't emit this yet — derived from counts on the client. */
  status?: string;
}

interface KernelProcess {
  pid: string;
  name: string;
  state: string;
  priority?: string;
  created_at?: string;
}

interface CostBudget {
  daily_project_limit: number;
  daily_global_limit: number;
  max_tokens_per_call: number;
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

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

function formatDuration(ms: number): string {
  if (ms < 1000) return `${ms}ms`;
  if (ms < 60_000) return `${(ms / 1000).toFixed(1)}s`;
  return `${(ms / 60_000).toFixed(1)}m`;
}

function relativeTime(dateStr: string | null | undefined): string {
  if (!dateStr) return "—";
  const then = new Date(dateStr).getTime();
  if (Number.isNaN(then)) return "—";
  const diff = Date.now() - then;
  if (diff < 0) return "just now";
  if (diff < 60_000) return "just now";
  if (diff < 3_600_000) return `${Math.floor(diff / 60_000)}m ago`;
  if (diff < 86_400_000) return `${Math.floor(diff / 3_600_000)}h ago`;
  return `${Math.floor(diff / 86_400_000)}d ago`;
}

function statusColor(status: string | undefined): string {
  if (!status) return "text-slate-400";
  const s = status.toLowerCase();
  if (s === "success" || s === "completed" || s === "running" || s === "healthy")
    return "text-emerald-400";
  if (s === "failed" || s === "error" || s === "unhealthy") return "text-red-400";
  if (s === "pending" || s === "suspended" || s === "degraded") return "text-amber-400";
  return "text-slate-400";
}

function statusBadgeVariant(status: string | undefined): "default" | "secondary" | "destructive" | "outline" {
  const s = (status ?? "").toLowerCase();
  if (s === "success" || s === "completed" || s === "running") return "default";
  if (s === "failed" || s === "error") return "destructive";
  return "secondary";
}

// ---------------------------------------------------------------------------
// Metric Card
// ---------------------------------------------------------------------------

function MetricCard({
  label,
  value,
  icon: Icon,
  color,
  sub,
  index,
}: {
  label: string;
  value: string;
  icon: typeof Activity;
  color: string;
  sub?: string;
  index: number;
}) {
  return (
    <StaggerItem index={index}>
      <Card>
        <CardContent className="p-4 pt-4">
          <div className="flex items-center gap-3">
            <div
              className={cn(
                "w-9 h-9 rounded-lg flex items-center justify-center shrink-0",
                color.replace("text-", "bg-").replace("400", "500/10")
              )}
            >
              <Icon className={cn("h-4.5 w-4.5", color)} />
            </div>
            <div className="min-w-0">
              <p className="text-[11px] text-slate-400 truncate">{label}</p>
              <p className="text-xl font-bold tabular-nums">{value}</p>
              {sub && <p className="text-[10px] text-slate-500 mt-0.5">{sub}</p>}
            </div>
          </div>
        </CardContent>
      </Card>
    </StaggerItem>
  );
}

// ---------------------------------------------------------------------------
// Trace Row
// ---------------------------------------------------------------------------

function TraceRow({ trace }: { trace: Trace }) {
  const [expanded, setExpanded] = useState(false);

  return (
    <div className="border-b border-white/[0.04] last:border-0">
      <button
        onClick={() => setExpanded(!expanded)}
        className="w-full flex items-center gap-3 py-2.5 px-3 hover:bg-white/[0.02] transition-colors text-left"
      >
        {expanded ? (
          <ChevronDown className="w-3.5 h-3.5 text-slate-500 shrink-0" />
        ) : (
          <ChevronRight className="w-3.5 h-3.5 text-slate-500 shrink-0" />
        )}

        <div className="flex-1 min-w-0 flex items-center gap-2">
          <span className="text-xs text-slate-200 font-medium truncate">
            {trace.agent_name || trace.action || "trace"}
          </span>
          {trace.status && (
            <Badge
              variant={statusBadgeVariant(trace.status)}
              className="text-[9px] px-1.5 py-0"
            >
              {trace.status}
            </Badge>
          )}
        </div>

        <div className="flex items-center gap-4 shrink-0">
          {trace.duration_ms !== undefined && (
            <span className="text-[11px] text-slate-400 tabular-nums flex items-center gap-1">
              <Clock className="w-3 h-3" />
              {formatDuration(trace.duration_ms)}
            </span>
          )}
          {trace.cost_usd !== undefined && trace.cost_usd > 0 && (
            <span className="text-[11px] text-slate-400 tabular-nums">
              {formatUsd(trace.cost_usd)}
            </span>
          )}
          <span className="text-[10px] text-slate-500 w-14 text-right">
            {relativeTime(trace.created_at)}
          </span>
        </div>
      </button>

      {expanded && (
        <div className="px-9 pb-3 space-y-1.5 animate-fade-in-up">
          <div className="grid grid-cols-2 md:grid-cols-4 gap-2 text-[11px]">
            <div>
              <span className="text-slate-500">ID:</span>{" "}
              <span className="text-slate-300 font-mono">{(trace.id ?? "").slice(0, 12)}</span>
            </div>
            {trace.tokens_used !== undefined && (
              <div>
                <span className="text-slate-500">Tokens:</span>{" "}
                <span className="text-slate-300 tabular-nums">{formatTokens(trace.tokens_used)}</span>
              </div>
            )}
            {trace.agent_name && (
              <div>
                <span className="text-slate-500">Agent:</span>{" "}
                <span className="text-slate-300">{trace.agent_name}</span>
              </div>
            )}
            {trace.action && (
              <div>
                <span className="text-slate-500">Action:</span>{" "}
                <span className="text-slate-300">{trace.action}</span>
              </div>
            )}
          </div>
          {trace.detail && (
            <p className="text-[11px] text-slate-400 bg-white/[0.02] rounded-lg p-2 font-mono whitespace-pre-wrap break-all">
              {trace.detail}
            </p>
          )}
        </div>
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Cost Breakdown Table
// ---------------------------------------------------------------------------

function CostBreakdownTable({ records }: { records: CostRecord[] }) {
  // Group by model
  const byModel = new Map<string, { cost: number; tokens: number; calls: number }>();
  for (const r of records) {
    const model = r.model || "unknown";
    const existing = byModel.get(model) || { cost: 0, tokens: 0, calls: 0 };
    existing.cost += r.cost_usd || 0;
    existing.tokens += r.total_tokens || 0;
    existing.calls += 1;
    byModel.set(model, existing);
  }

  const rows = Array.from(byModel.entries())
    .sort((a, b) => b[1].cost - a[1].cost);

  const totalCost = rows.reduce((sum, [, v]) => sum + v.cost, 0);

  if (rows.length === 0) {
    return (
      <div className="py-6 text-center">
        <BarChart3 className="w-6 h-6 text-slate-400/30 mx-auto mb-1.5" />
        <p className="text-xs text-slate-500">No cost data yet</p>
      </div>
    );
  }

  return (
    <div className="overflow-x-auto">
      <table className="w-full">
        <thead>
          <tr className="border-b border-white/[0.06]">
            <th className="text-left px-4 py-2.5 text-[11px] font-medium text-slate-400">Model</th>
            <th className="text-right px-4 py-2.5 text-[11px] font-medium text-slate-400">Calls</th>
            <th className="text-right px-4 py-2.5 text-[11px] font-medium text-slate-400">Tokens</th>
            <th className="text-right px-4 py-2.5 text-[11px] font-medium text-slate-400">Cost</th>
            <th className="text-right px-4 py-2.5 text-[11px] font-medium text-slate-400">Share</th>
          </tr>
        </thead>
        <tbody>
          {rows.map(([model, data]) => (
            <tr
              key={model}
              className="border-b border-white/[0.04] last:border-0 hover:bg-white/[0.02]"
            >
              <td className="px-4 py-2 text-[12px] font-medium text-slate-200">{model}</td>
              <td className="px-4 py-2 text-[12px] text-slate-400 text-right tabular-nums">
                {data.calls}
              </td>
              <td className="px-4 py-2 text-[12px] text-slate-400 text-right tabular-nums">
                {formatTokens(data.tokens)}
              </td>
              <td className="px-4 py-2 text-[12px] text-slate-200 text-right font-medium tabular-nums">
                {formatUsd(data.cost)}
              </td>
              <td className="px-4 py-2 text-[12px] text-slate-400 text-right tabular-nums">
                {totalCost > 0 ? `${Math.round((data.cost / totalCost) * 100)}%` : "0%"}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Budget Bar
// ---------------------------------------------------------------------------

function BudgetBar({
  used,
  limit,
  label,
}: {
  used: number;
  limit: number;
  label: string;
}) {
  const pct = limit > 0 ? Math.min((used / limit) * 100, 100) : 0;
  const barColor =
    pct >= 90 ? "bg-red-400" : pct >= 70 ? "bg-amber-400" : "bg-emerald-400";
  const textColor =
    pct >= 90 ? "text-red-400" : pct >= 70 ? "text-amber-400" : "text-emerald-400";

  return (
    <div className="space-y-1.5">
      <div className="flex items-center justify-between text-[11px]">
        <span className="text-slate-400">{label}</span>
        <span className={cn("font-medium tabular-nums", textColor)}>
          {formatUsd(used)} / {formatUsd(limit)}
        </span>
      </div>
      <div className="w-full h-1.5 rounded-full bg-white/[0.06] overflow-hidden">
        <div
          className={cn("h-full rounded-full transition-all duration-500", barColor)}
          style={{ width: `${pct}%` }}
        />
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Page
// ---------------------------------------------------------------------------

export default function ObservabilityPage() {
  const params = useParams();
  const projectId = params.projectId as string;

  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [refreshing, setRefreshing] = useState(false);
  const [failedSections, setFailedSections] = useState<string[]>([]);

  // Data
  const [costSummary, setCostSummary] = useState<CostSummary | null>(null);
  const [traces, setTraces] = useState<Trace[]>([]);
  const [runMetrics, setRunMetrics] = useState<RunMetric[]>([]);
  const [memoryHealth, setMemoryHealth] = useState<MemoryHealth | null>(null);
  const [kernelProcesses, setKernelProcesses] = useState<KernelProcess[]>([]);
  const [budget, setBudget] = useState<CostBudget | null>(null);

  const fetchAll = useCallback(async () => {
    const failed: string[] = [];
    const ok = async <T,>(
      label: string,
      res: PromiseSettledResult<Response>,
      apply: (data: unknown) => void,
    ): Promise<void> => {
      if (res.status !== "fulfilled") {
        failed.push(label);
        return;
      }
      if (!res.value.ok) {
        failed.push(`${label} (HTTP ${res.value.status})`);
        return;
      }
      try {
        const data = (await res.value.json()) as T;
        apply(data);
      } catch {
        failed.push(`${label} (parse)`);
      }
    };

    try {
      const [costRes, tracesRes, metricsRes, memRes, kernelRes, budgetRes] =
        await Promise.allSettled([
          fetch(`${BASE}/costs/summary`),
          fetch(`${BASE}/projects/${projectId}/traces?limit=50`),
          fetch(`${BASE}/projects/${projectId}/metrics`),
          fetch(`${BASE}/memory/health`),
          fetch(`${BASE}/kernel/processes`),
          fetch(`${BASE}/costs/budget`),
        ]);

      await ok<CostSummary>("Cost summary", costRes, (d) =>
        setCostSummary(d as CostSummary),
      );
      await ok<Trace[] | { traces?: Trace[] }>("Traces", tracesRes, (d) => {
        setTraces(Array.isArray(d) ? d : (d as { traces?: Trace[] }).traces ?? []);
      });
      await ok<RunMetric[]>("Run metrics", metricsRes, (d) => {
        setRunMetrics(Array.isArray(d) ? d : []);
      });
      await ok<MemoryHealth>("Memory health", memRes, (d) => {
        const raw = d as MemoryHealth;
        const status =
          raw.status ?? (raw.episode_count + raw.fact_count > 0 ? "healthy" : "idle");
        setMemoryHealth({ ...raw, status });
      });
      await ok<KernelProcess[] | { processes?: KernelProcess[] }>(
        "Kernel processes",
        kernelRes,
        (d) => {
          setKernelProcesses(
            Array.isArray(d)
              ? d
              : (d as { processes?: KernelProcess[] }).processes ?? [],
          );
        },
      );
      await ok<CostBudget>("Budget", budgetRes, (d) => setBudget(d as CostBudget));

      setFailedSections(failed);
      setError(null);
    } catch (err) {
      setError(
        err instanceof Error ? err.message : "Failed to load observability data",
      );
    } finally {
      setLoading(false);
      setRefreshing(false);
    }
  }, [projectId]);

  useEffect(() => {
    fetchAll();
  }, [fetchAll]);

  // Auto-refresh every 10s — paused while the tab is hidden.
  useVisibleInterval(fetchAll, 10_000, { runOnShow: true });

  const handleRefresh = () => {
    setRefreshing(true);
    fetchAll();
  };

  // ---------------------------------------------------------------------------
  // Derived metrics
  // ---------------------------------------------------------------------------

  const totalCalls = costSummary?.total_calls ?? 0;
  const totalTokens = costSummary?.total_tokens ?? 0;
  const totalCost = costSummary?.total_cost_usd ?? 0;

  const avgResponseTime =
    runMetrics.length > 0
      ? runMetrics.reduce((sum, m) => sum + (m.duration_ms ?? 0), 0) / runMetrics.length
      : 0;

  const runningProcesses = kernelProcesses.filter(
    (p) => (p.state ?? "").toLowerCase() === "running"
  ).length;
  const completedProcesses = kernelProcesses.filter(
    (p) => (p.state ?? "").toLowerCase() === "completed"
  ).length;
  const failedProcesses = kernelProcesses.filter(
    (p) => (p.state ?? "").toLowerCase() === "failed"
  ).length;

  // ---------------------------------------------------------------------------
  // Loading skeleton
  // ---------------------------------------------------------------------------

  if (loading) {
    return (
      <div className="p-6 space-y-6">
        <div className="flex items-center gap-3">
          <Skeleton className="h-9 w-9 rounded-lg" />
          <div className="space-y-1">
            <Skeleton className="h-5 w-48" />
            <Skeleton className="h-3 w-64" />
          </div>
        </div>
        <div className="grid grid-cols-2 md:grid-cols-3 lg:grid-cols-6 gap-4">
          {Array.from({ length: 6 }).map((_, i) => (
            <Skeleton key={i} className="h-24 rounded-xl" />
          ))}
        </div>
        <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
          <Skeleton className="h-80 rounded-xl" />
          <Skeleton className="h-80 rounded-xl" />
        </div>
        <Skeleton className="h-48 rounded-xl" />
      </div>
    );
  }

  // ---------------------------------------------------------------------------
  // Render
  // ---------------------------------------------------------------------------

  return (
    <div className="flex flex-col gap-6 p-6 overflow-auto h-full">
      {/* Header */}
      <PageHeader
        title="Observability"
        subtitle="System metrics, traces, costs, and health"
        actions={
          <button
            onClick={handleRefresh}
            disabled={refreshing}
            className="flex items-center gap-1.5 px-3 py-1.5 text-xs font-medium rounded-lg bg-white/[0.04] border border-white/[0.08] text-slate-300 hover:bg-white/[0.08] transition-colors disabled:opacity-50"
          >
            <RefreshCw
              className={cn("w-3.5 h-3.5", refreshing && "animate-spin")}
            />
            Refresh
          </button>
        }
      />

      {/* Error banner */}
      {error && (
        <div className="rounded-lg border border-red-500/20 bg-red-500/5 p-4 text-sm text-red-400 animate-fade-in-up">
          {error}
        </div>
      )}

      {/* Partial failure banner */}
      {failedSections.length > 0 && (
        <div className="rounded-lg border border-amber-500/20 bg-amber-500/5 p-3 text-xs text-amber-300 animate-fade-in-up">
          <span className="font-medium">Some sections failed to load:</span>{" "}
          {failedSections.join(", ")}. Data shown may be stale.
        </div>
      )}

      {/* Metric cards */}
      <div className="grid grid-cols-2 md:grid-cols-3 lg:grid-cols-6 gap-4">
        <MetricCard
          label="LLM Calls"
          value={String(totalCalls)}
          icon={Zap}
          color="text-sky-400"
          sub="today"
          index={0}
        />
        <MetricCard
          label="Total Tokens"
          value={formatTokens(totalTokens)}
          icon={Activity}
          color="text-violet-400"
          sub="today"
          index={1}
        />
        <MetricCard
          label="Avg Response"
          value={avgResponseTime > 0 ? formatDuration(avgResponseTime) : "--"}
          icon={Clock}
          color="text-amber-400"
          sub={runMetrics.length > 0 ? `${runMetrics.length} runs` : undefined}
          index={2}
        />
        <MetricCard
          label="Cost to Date"
          value={formatUsd(totalCost)}
          icon={DollarSign}
          color="text-emerald-400"
          sub="today"
          index={3}
        />
        <MetricCard
          label="Active Processes"
          value={String(runningProcesses)}
          icon={Cpu}
          color="text-cyan-400"
          sub={
            kernelProcesses.length > 0
              ? `${completedProcesses} done, ${failedProcesses} failed`
              : undefined
          }
          index={4}
        />
        <MetricCard
          label="Memory Health"
          value={
            memoryHealth
              ? `${memoryHealth.episode_count + memoryHealth.fact_count}`
              : "--"
          }
          icon={Brain}
          color="text-pink-400"
          sub={
            memoryHealth
              ? `${memoryHealth.episode_count} episodes, ${memoryHealth.fact_count} facts`
              : undefined
          }
          index={5}
        />
      </div>

      {/* Main content: Traces + Cost */}
      <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
        {/* Recent Traces Timeline */}
        <StaggerItem index={6}>
          <Card className="h-full">
            <CardHeader className="border-b border-white/[0.06]">
              <CardTitle className="text-sm font-medium text-slate-200 flex items-center gap-2">
                <Activity className="w-4 h-4 text-slate-400" />
                Recent Traces
                {traces.length > 0 && (
                  <Badge variant="secondary" className="text-[10px]">
                    {traces.length}
                  </Badge>
                )}
                <span className="ml-auto flex items-center gap-1 text-[10px] text-slate-500 font-normal">
                  <span className="w-1.5 h-1.5 rounded-full bg-emerald-400 animate-pulse" />
                  Auto-refresh 10s
                </span>
              </CardTitle>
            </CardHeader>
            <CardContent className="p-0">
              {traces.length > 0 ? (
                <div className="max-h-[400px] overflow-y-auto scrollbar-thin">
                  {traces.map((trace, i) => (
                    <TraceRow key={trace.id ?? i} trace={trace} />
                  ))}
                </div>
              ) : (
                <div className="py-10 text-center">
                  <Activity className="w-8 h-8 text-slate-400/20 mx-auto mb-2" />
                  <p className="text-xs text-slate-500">No traces recorded yet</p>
                  <p className="text-[10px] text-slate-600 mt-0.5">
                    Traces will appear as agents execute tasks
                  </p>
                </div>
              )}
            </CardContent>
          </Card>
        </StaggerItem>

        {/* Cost Breakdown */}
        <StaggerItem index={7}>
          <Card className="h-full">
            <CardHeader className="border-b border-white/[0.06]">
              <CardTitle className="text-sm font-medium text-slate-200 flex items-center gap-2">
                <BarChart3 className="w-4 h-4 text-slate-400" />
                Cost Breakdown by Model
              </CardTitle>
            </CardHeader>
            <CardContent className="p-0">
              <CostBreakdownTable records={costSummary?.records ?? []} />
            </CardContent>

            {/* Budget utilization */}
            {budget && (
              <div className="px-5 pb-5 space-y-3">
                <div className="border-t border-white/[0.06] pt-4">
                  <p className="text-[11px] text-slate-400 font-medium mb-3 flex items-center gap-1.5">
                    <TrendingUp className="w-3.5 h-3.5" />
                    Budget Utilization
                  </p>
                  <BudgetBar
                    used={totalCost}
                    limit={budget.daily_project_limit}
                    label="Daily project limit"
                  />
                  <div className="mt-2">
                    <BudgetBar
                      used={totalCost}
                      limit={budget.daily_global_limit}
                      label="Daily global limit"
                    />
                  </div>
                </div>
              </div>
            )}
          </Card>
        </StaggerItem>
      </div>

      {/* System Health row */}
      <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-6">
        {/* Kernel Processes */}
        <StaggerItem index={8}>
          <Card>
            <CardHeader className="border-b border-white/[0.06]">
              <CardTitle className="text-sm font-medium text-slate-200 flex items-center gap-2">
                <Server className="w-4 h-4 text-slate-400" />
                Kernel Processes
              </CardTitle>
            </CardHeader>
            <CardContent className="pt-4">
              {kernelProcesses.length > 0 ? (
                <>
                  {/* Summary */}
                  <div className="flex items-center gap-4 mb-3">
                    <div className="flex items-center gap-1.5">
                      <span className="w-2 h-2 rounded-full bg-emerald-400" />
                      <span className="text-[11px] text-slate-400">
                        Running: <span className="text-slate-200 font-medium">{runningProcesses}</span>
                      </span>
                    </div>
                    <div className="flex items-center gap-1.5">
                      <span className="w-2 h-2 rounded-full bg-slate-500" />
                      <span className="text-[11px] text-slate-400">
                        Done: <span className="text-slate-200 font-medium">{completedProcesses}</span>
                      </span>
                    </div>
                    <div className="flex items-center gap-1.5">
                      <span className="w-2 h-2 rounded-full bg-red-400" />
                      <span className="text-[11px] text-slate-400">
                        Failed: <span className="text-slate-200 font-medium">{failedProcesses}</span>
                      </span>
                    </div>
                  </div>

                  {/* Process list */}
                  <div className="max-h-48 overflow-y-auto space-y-1.5 scrollbar-thin">
                    {kernelProcesses.slice(0, 10).map((proc) => (
                      <div
                        key={proc.pid}
                        className="flex items-center justify-between py-1.5 px-2 rounded-lg bg-white/[0.02] border border-white/[0.04]"
                      >
                        <div className="flex items-center gap-2 min-w-0">
                          <span
                            className={cn(
                              "w-1.5 h-1.5 rounded-full shrink-0",
                              (proc.state ?? "").toLowerCase() === "running"
                                ? "bg-emerald-400"
                                : (proc.state ?? "").toLowerCase() === "failed"
                                  ? "bg-red-400"
                                  : "bg-slate-500"
                            )}
                          />
                          <span className="text-[11px] text-slate-200 truncate">
                            {proc.name}
                          </span>
                        </div>
                        <Badge
                          variant="outline"
                          className={cn("text-[9px] shrink-0", statusColor(proc.state))}
                        >
                          {proc.state}
                        </Badge>
                      </div>
                    ))}
                  </div>
                </>
              ) : (
                <div className="py-6 text-center">
                  <Server className="w-6 h-6 text-slate-400/20 mx-auto mb-1.5" />
                  <p className="text-xs text-slate-500">No processes</p>
                </div>
              )}
            </CardContent>
          </Card>
        </StaggerItem>

        {/* Memory System Health */}
        <StaggerItem index={9}>
          <Card>
            <CardHeader className="border-b border-white/[0.06]">
              <CardTitle className="text-sm font-medium text-slate-200 flex items-center gap-2">
                <Brain className="w-4 h-4 text-slate-400" />
                Memory System
              </CardTitle>
            </CardHeader>
            <CardContent className="pt-4">
              {memoryHealth ? (
                <div className="space-y-4">
                  {/* Status */}
                  <div className="flex items-center gap-2">
                    {memoryHealth.status === "healthy" ? (
                      <CheckCircle2 className="w-4 h-4 text-emerald-400" />
                    ) : (
                      <XCircle className="w-4 h-4 text-amber-400" />
                    )}
                    <span
                      className={cn(
                        "text-sm font-medium capitalize",
                        statusColor(memoryHealth.status)
                      )}
                    >
                      {memoryHealth.status}
                    </span>
                  </div>

                  {/* Counters */}
                  <div className="grid grid-cols-2 gap-3">
                    <div className="p-3 rounded-lg bg-white/[0.02] border border-white/[0.04]">
                      <p className="text-[10px] text-slate-500 uppercase tracking-wider">Episodes</p>
                      <p className="text-lg font-bold text-slate-200 tabular-nums mt-0.5">
                        {memoryHealth.episode_count}
                      </p>
                    </div>
                    <div className="p-3 rounded-lg bg-white/[0.02] border border-white/[0.04]">
                      <p className="text-[10px] text-slate-500 uppercase tracking-wider">Facts</p>
                      <p className="text-lg font-bold text-slate-200 tabular-nums mt-0.5">
                        {memoryHealth.fact_count}
                      </p>
                    </div>
                  </div>

                  {/* Last consolidation */}
                  {memoryHealth.last_consolidation && (
                    <div className="flex items-center gap-1.5 text-[11px] text-slate-500">
                      <Sparkles className="w-3 h-3" />
                      Last consolidation:{" "}
                      {relativeTime(memoryHealth.last_consolidation)}
                    </div>
                  )}
                </div>
              ) : (
                <div className="py-6 text-center">
                  <Brain className="w-6 h-6 text-slate-400/20 mx-auto mb-1.5" />
                  <p className="text-xs text-slate-500">Memory system not available</p>
                </div>
              )}
            </CardContent>
          </Card>
        </StaggerItem>

        {/* Agent / Run Metrics Summary */}
        <StaggerItem index={10}>
          <Card>
            <CardHeader className="border-b border-white/[0.06]">
              <CardTitle className="text-sm font-medium text-slate-200 flex items-center gap-2">
                <Heart className="w-4 h-4 text-slate-400" />
                Run Metrics
              </CardTitle>
            </CardHeader>
            <CardContent className="pt-4">
              {runMetrics.length > 0 ? (
                <div className="space-y-4">
                  {/* Aggregated stats */}
                  <div className="grid grid-cols-3 gap-3">
                    <div className="text-center p-2 rounded-lg bg-white/[0.02] border border-white/[0.04]">
                      <p className="text-lg font-bold text-emerald-400 tabular-nums">
                        {runMetrics.filter((m) => m.status === "completed" || m.status === "success").length}
                      </p>
                      <p className="text-[10px] text-slate-500">Success</p>
                    </div>
                    <div className="text-center p-2 rounded-lg bg-white/[0.02] border border-white/[0.04]">
                      <p className="text-lg font-bold text-red-400 tabular-nums">
                        {runMetrics.filter((m) => m.status === "failed" || m.status === "error").length}
                      </p>
                      <p className="text-[10px] text-slate-500">Failed</p>
                    </div>
                    <div className="text-center p-2 rounded-lg bg-white/[0.02] border border-white/[0.04]">
                      <p className="text-lg font-bold text-slate-200 tabular-nums">
                        {runMetrics.length}
                      </p>
                      <p className="text-[10px] text-slate-500">Total</p>
                    </div>
                  </div>

                  {/* Recent runs */}
                  <div className="max-h-40 overflow-y-auto space-y-1 scrollbar-thin">
                    {runMetrics.slice(0, 8).map((m) => (
                      <div
                        key={m.id}
                        className="flex items-center justify-between py-1.5 border-b border-white/[0.04] last:border-0"
                      >
                        <div className="flex items-center gap-2 min-w-0">
                          <span
                            className={cn(
                              "w-1.5 h-1.5 rounded-full shrink-0",
                              m.status === "completed" || m.status === "success"
                                ? "bg-emerald-400"
                                : m.status === "failed" || m.status === "error"
                                  ? "bg-red-400"
                                  : "bg-amber-400"
                            )}
                          />
                          <span className="text-[11px] text-slate-300 font-mono truncate">
                            {(m.run_id ?? "").slice(0, 10)}
                          </span>
                        </div>
                        <span className="text-[10px] text-slate-500">
                          {m.duration_ms ? formatDuration(m.duration_ms) : "--"}
                        </span>
                      </div>
                    ))}
                  </div>
                </div>
              ) : (
                <div className="py-6 text-center">
                  <Heart className="w-6 h-6 text-slate-400/20 mx-auto mb-1.5" />
                  <p className="text-xs text-slate-500">No run metrics yet</p>
                  <p className="text-[10px] text-slate-600 mt-0.5">
                    Metrics appear after agent runs complete
                  </p>
                </div>
              )}
            </CardContent>
          </Card>
        </StaggerItem>
      </div>
    </div>
  );
}
