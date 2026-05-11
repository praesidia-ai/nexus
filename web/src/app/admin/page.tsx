"use client";

import { useState, useEffect, useCallback } from "react";
import Link from "next/link";
import {
  ArrowLeft, Activity, Search, DollarSign, Shield, Cpu, Loader2,
  Eye, Zap, FlaskConical, Key, Server, Globe, Webhook,
  Plus, Trash2, Play, Pause, RotateCcw, RefreshCw, CheckCircle2,
  XCircle, AlertTriangle, Clock, TrendingDown, ChevronDown, ChevronRight,
  Send, Link2, Unlink, CircleDot,
} from "lucide-react";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Progress } from "@/components/ui/progress";
import { Skeleton } from "@/components/ui/skeleton";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { useToast } from "@/components/toast";

/**
 * Dispatch an admin-action error to the root AdminPage listener, which turns
 * it into a toast. Kept as a plain function (not a hook) so it can be called
 * from any sub-component without threading `toast` through every panel.
 */
function reportAdminError(err: unknown): void {
  if (typeof window === "undefined") return;
  console.error("admin action failed", err);
  const message = err instanceof Error ? err.message : String(err);
  window.dispatchEvent(
    new CustomEvent("nexus:admin-error", { detail: { message } }),
  );
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

const BASE = "/api";

async function get<T = unknown>(path: string): Promise<T> {
  const res = await fetch(BASE + path);
  if (!res.ok) throw new Error(`GET ${path} → ${res.status}`);
  return res.json();
}

async function post<T = unknown>(path: string, body?: unknown): Promise<T> {
  const res = await fetch(BASE + path, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: body != null ? JSON.stringify(body) : undefined,
  });
  if (!res.ok) throw new Error(`POST ${path} → ${res.status}`);
  return res.json();
}

async function del(path: string): Promise<void> {
  const res = await fetch(BASE + path, { method: "DELETE" });
  if (!res.ok) throw new Error(`DELETE ${path} → ${res.status}`);
}

function EmptyState({ icon: Icon, title, subtitle }: { icon: React.ElementType; title: string; subtitle: string }) {
  return (
    <Card className="border-dashed">
      <CardContent className="p-12 flex flex-col items-center justify-center text-center">
        <Icon className="h-12 w-12 text-slate-400/30 mb-4" />
        <h3 className="text-lg font-semibold text-slate-200">{title}</h3>
        <p className="text-sm text-slate-400 mt-1">{subtitle}</p>
      </CardContent>
    </Card>
  );
}

function LoadingSkeleton({ rows = 3 }: { rows?: number }) {
  return (
    <div className="space-y-3">
      {Array.from({ length: rows }).map((_, i) => (
        <Skeleton key={i} className="h-12 w-full" />
      ))}
    </div>
  );
}

function StatusDot({ ok }: { ok: boolean }) {
  return <span className={`h-2 w-2 rounded-full inline-block ${ok ? "bg-emerald-400" : "bg-red-400"}`} />;
}

function useData<T>(fetcher: () => Promise<T>, deps: unknown[] = []) {
  const [data, setData] = useState<T | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const reload = useCallback(() => {
    setLoading(true);
    setError(null);
    fetcher()
      .then(setData)
      .catch((e) => setError(e.message))
      .finally(() => setLoading(false));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, deps);
  useEffect(() => { reload(); }, [reload]);
  return { data, loading, error, reload, setData };
}

// ---------------------------------------------------------------------------
// Page root
// ---------------------------------------------------------------------------

export default function AdminPage() {
  const { toast } = useToast();
  useEffect(() => {
    function onAdminError(ev: Event) {
      const detail = (ev as CustomEvent<{ message: string }>).detail;
      toast("error", "Action failed", detail?.message ?? "Unknown error");
    }
    window.addEventListener("nexus:admin-error", onAdminError);
    return () => window.removeEventListener("nexus:admin-error", onAdminError);
  }, [toast]);

  return (
    <div className="h-screen overflow-y-auto scrollbar-thin p-8 max-w-7xl mx-auto">
      <Link
        href="/"
        className="inline-flex items-center gap-1.5 text-sm text-slate-400 hover:text-slate-200 transition-colors mb-6 focus-visible:outline-none focus-visible:text-glow-cyan"
      >
        <ArrowLeft className="w-4 h-4" />
        Back to Home
      </Link>

      <div className="mb-6">
        <h1 className="text-2xl font-bold text-slate-200 mb-1">Admin</h1>
        <p className="text-sm text-slate-400">System observability, costs, and operational controls</p>
      </div>

      <Tabs defaultValue="health" className="space-y-6">
        <TabsList className="flex-wrap h-auto gap-1 p-1.5">
          <TabsTrigger value="health"><Activity className="h-3.5 w-3.5 mr-1.5" />Health</TabsTrigger>
          <TabsTrigger value="traces"><Eye className="h-3.5 w-3.5 mr-1.5" />Traces</TabsTrigger>
          <TabsTrigger value="cost"><DollarSign className="h-3.5 w-3.5 mr-1.5" />Cost</TabsTrigger>
          <TabsTrigger value="control"><Shield className="h-3.5 w-3.5 mr-1.5" />Control</TabsTrigger>
          <TabsTrigger value="super-agents"><Cpu className="h-3.5 w-3.5 mr-1.5" />Super Agents</TabsTrigger>
          <TabsTrigger value="eval"><FlaskConical className="h-3.5 w-3.5 mr-1.5" />Eval</TabsTrigger>
          <TabsTrigger value="security"><Key className="h-3.5 w-3.5 mr-1.5" />Security</TabsTrigger>
          <TabsTrigger value="mcp"><Server className="h-3.5 w-3.5 mr-1.5" />MCP</TabsTrigger>
          <TabsTrigger value="federation"><Globe className="h-3.5 w-3.5 mr-1.5" />Federation</TabsTrigger>
          <TabsTrigger value="webhooks"><Webhook className="h-3.5 w-3.5 mr-1.5" />Webhooks</TabsTrigger>
        </TabsList>

        <TabsContent value="health"><HealthPanel /></TabsContent>
        <TabsContent value="traces"><TracesPanel /></TabsContent>
        <TabsContent value="cost"><CostPanel /></TabsContent>
        <TabsContent value="control"><ControlPanel /></TabsContent>
        <TabsContent value="super-agents"><SuperAgentsPanel /></TabsContent>
        <TabsContent value="eval"><EvalPanel /></TabsContent>
        <TabsContent value="security"><SecurityPanel /></TabsContent>
        <TabsContent value="mcp"><McpPanel /></TabsContent>
        <TabsContent value="federation"><FederationPanel /></TabsContent>
        <TabsContent value="webhooks"><WebhooksPanel /></TabsContent>
      </Tabs>
    </div>
  );
}

// ---------------------------------------------------------------------------
// 1. Health
// ---------------------------------------------------------------------------

function HealthPanel() {
  const { data: basic, loading: l1 } = useData(() => get<{ status: string; uptime_secs?: number }>("/health"));
  const { data: detailed, loading: l2 } = useData(() => get<Record<string, unknown>>("/health/detailed"));
  const { data: live, loading: l3 } = useData(() => get<{ status: string }>("/health/live"));
  const { data: ready, loading: l4 } = useData(() => get<{ status: string }>("/health/ready"));

  if (l1 || l2 || l3 || l4) return <LoadingSkeleton rows={4} />;

  const isHealthy = basic?.status === "ok" || basic?.status === "healthy";
  const uptimeHrs = basic?.uptime_secs ? Math.floor(basic.uptime_secs / 3600) : null;
  const uptimeMins = basic?.uptime_secs ? Math.floor((basic.uptime_secs % 3600) / 60) : null;

  return (
    <div className="space-y-4">
      <div className="grid grid-cols-1 sm:grid-cols-4 gap-4">
        <Card className={`border ${isHealthy ? "border-emerald-500/20 bg-emerald-500/5" : "border-red-500/20 bg-red-500/5"}`}>
          <CardContent className="p-4 text-center">
            <p className="text-xs text-slate-400">Overall</p>
            <div className="flex items-center justify-center gap-2 mt-1">
              {isHealthy ? <CheckCircle2 className="h-5 w-5 text-emerald-400" /> : <XCircle className="h-5 w-5 text-red-400" />}
              <p className={`text-xl font-bold ${isHealthy ? "text-emerald-400" : "text-red-400"}`}>
                {isHealthy ? "Healthy" : "Degraded"}
              </p>
            </div>
          </CardContent>
        </Card>
        <Card>
          <CardContent className="p-4 text-center">
            <p className="text-xs text-slate-400">Uptime</p>
            <p className="text-xl font-bold text-slate-200 mt-1">{uptimeHrs != null ? `${uptimeHrs}h ${uptimeMins}m` : "--"}</p>
          </CardContent>
        </Card>
        <Card>
          <CardContent className="p-4 text-center">
            <p className="text-xs text-slate-400">Liveness</p>
            <Badge variant={live?.status === "ok" || live?.status === "alive" ? "success" : "destructive"} className="mt-2">
              {live?.status ?? "unknown"}
            </Badge>
          </CardContent>
        </Card>
        <Card>
          <CardContent className="p-4 text-center">
            <p className="text-xs text-slate-400">Readiness</p>
            <Badge variant={ready?.status === "ok" || ready?.status === "ready" ? "success" : "warning"} className="mt-2">
              {ready?.status ?? "unknown"}
            </Badge>
          </CardContent>
        </Card>
      </div>

      {detailed && (
        <Card>
          <CardHeader className="border-b border-white/[0.06]">
            <CardTitle className="text-sm">Detailed Health</CardTitle>
          </CardHeader>
          <CardContent className="p-4">
            <div className="space-y-2">
              {Object.entries(detailed).map(([key, val]) => (
                <div key={key} className="flex items-center justify-between py-1.5 border-b border-white/[0.04] last:border-0">
                  <span className="text-sm text-slate-400">{key}</span>
                  <span className="text-sm text-slate-200 font-mono">{typeof val === "object" ? JSON.stringify(val) : String(val)}</span>
                </div>
              ))}
            </div>
          </CardContent>
        </Card>
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------
// 2. Traces
// ---------------------------------------------------------------------------

interface Trace { id: string; type?: string; status?: string; duration_ms?: number; timestamp?: string; metadata?: Record<string, unknown> }

function TracesPanel() {
  const [search, setSearch] = useState("");
  const [expanded, setExpanded] = useState<string | null>(null);
  const { data, loading, reload } = useData(() => get<{ traces?: Trace[] }>("/observability/traces"));
  const traces = data?.traces ?? (Array.isArray(data) ? data as Trace[] : []);
  const filtered = search ? traces.filter((t) => t.id.includes(search) || t.type?.includes(search) || t.status?.includes(search)) : traces;

  return (
    <div className="space-y-4">
      <div className="flex items-center gap-3">
        <div className="relative flex-1 max-w-sm">
          <Search className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-slate-400" />
          <input
            type="text" value={search} onChange={(e) => setSearch(e.target.value)}
            placeholder="Search traces..."
            aria-label="Search execution traces"
            className="w-full pl-9 pr-3 py-2 text-sm bg-white/[0.03] border border-white/[0.08] rounded-lg text-slate-200 placeholder:text-slate-400/50 focus:outline-none focus:border-glow-cyan/40"
          />
        </div>
        <Button variant="outline" size="sm" onClick={reload}><RefreshCw className="h-3.5 w-3.5 mr-1.5" />Refresh</Button>
      </div>

      {loading ? <LoadingSkeleton rows={5} /> : filtered.length === 0 ? (
        <EmptyState icon={Eye} title="No traces" subtitle="No execution traces recorded yet" />
      ) : (
        <Card>
          <CardContent className="p-0 divide-y divide-white/[0.04]">
            {filtered.map((t, i) => {
              const isExpanded = expanded === (t.id ?? String(i));
              const maxDur = Math.max(...traces.map((x) => x.duration_ms ?? 0), 1);
              return (
                <div key={t.id ?? i}>
                  <button
                    onClick={() => setExpanded(isExpanded ? null : (t.id ?? String(i)))}
                    className="w-full flex items-center gap-3 px-4 py-2.5 hover:bg-white/[0.02] transition-colors text-left"
                  >
                    {isExpanded ? <ChevronDown className="h-3.5 w-3.5 text-slate-400 shrink-0" /> : <ChevronRight className="h-3.5 w-3.5 text-slate-400 shrink-0" />}
                    <span className="text-xs font-medium text-slate-200 w-24 truncate">{t.type ?? "trace"}</span>
                    <Badge variant={t.status === "success" ? "success" : t.status === "error" ? "destructive" : "secondary"} className="text-[10px]">{t.status ?? "unknown"}</Badge>
                    <div className="flex-1 mx-4">
                      {t.duration_ms != null && (
                        <div className="h-1.5 rounded-full bg-white/[0.06] overflow-hidden">
                          <div className="h-full rounded-full bg-gradient-to-r from-glow-cyan to-glow-blue" style={{ width: `${Math.max((t.duration_ms / maxDur) * 100, 2)}%` }} />
                        </div>
                      )}
                    </div>
                    <span className="text-xs text-slate-400 tabular-nums w-16 text-right">{t.duration_ms != null ? `${t.duration_ms}ms` : "--"}</span>
                    <span className="text-xs text-slate-400 w-20 text-right">{t.timestamp ? new Date(t.timestamp).toLocaleTimeString() : "--"}</span>
                  </button>
                  {isExpanded && (
                    <div className="px-4 pb-3 pl-12">
                      <pre className="text-xs text-slate-400 bg-white/[0.02] rounded-lg p-3 overflow-x-auto max-h-48">
                        {JSON.stringify(t.metadata ?? t, null, 2)}
                      </pre>
                    </div>
                  )}
                </div>
              );
            })}
          </CardContent>
        </Card>
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------
// 3. Cost
// ---------------------------------------------------------------------------

interface CostSummary { today?: number; week?: number; month?: number; total?: number; by_model?: Record<string, number>; by_project?: Record<string, number>; budget?: number; budget_used?: number }
interface CostRec { id?: string; title?: string; description?: string; savings?: number }

function CostPanel() {
  const { data: summary, loading: l1, reload: r1 } = useData(() => get<CostSummary>("/costs/summary"));
  const { data: recs, loading: l2 } = useData(() => get<{ recommendations?: CostRec[] }>("/costs/recommendations"));
  const { data: modelRoute, loading: l3 } = useData(() => get<Record<string, unknown>>("/costs/model-route"));
  const [budgetInput, setBudgetInput] = useState("");
  const [saving, setSaving] = useState(false);

  async function setBudget() {
    if (!budgetInput) return;
    setSaving(true);
    try { await post("/costs/budget", { budget: parseFloat(budgetInput) }); setBudgetInput(""); r1(); } catch (e) { reportAdminError(e); }
    finally { setSaving(false); }
  }

  const fmt = (v?: number) => v != null ? `$${v.toFixed(2)}` : "--";
  const budgetPct = summary?.budget ? Math.min(((summary.budget_used ?? 0) / summary.budget) * 100, 100) : 0;
  const recommendations = recs?.recommendations ?? (Array.isArray(recs) ? recs as CostRec[] : []);

  if (l1 && l2 && l3) return <LoadingSkeleton rows={4} />;

  return (
    <div className="space-y-4">
      {/* Cost cards */}
      <div className="grid grid-cols-1 sm:grid-cols-4 gap-4">
        {[
          { label: "Today", value: summary?.today, color: "text-glow-cyan" },
          { label: "This Week", value: summary?.week, color: "text-blue-400" },
          { label: "This Month", value: summary?.month, color: "text-purple-400" },
          { label: "All Time", value: summary?.total, color: "text-slate-200" },
        ].map((c) => (
          <Card key={c.label}>
            <CardContent className="p-4 text-center">
              <p className="text-xs text-slate-400">{c.label}</p>
              <p className={`text-2xl font-bold mt-1 ${c.color}`}>{l1 ? "..." : fmt(c.value)}</p>
            </CardContent>
          </Card>
        ))}
      </div>

      {/* Budget */}
      <Card>
        <CardHeader className="border-b border-white/[0.06]">
          <CardTitle className="text-sm">Budget</CardTitle>
        </CardHeader>
        <CardContent className="p-4 space-y-3">
          {summary?.budget ? (
            <>
              <div className="flex justify-between text-sm">
                <span className="text-slate-400">{fmt(summary.budget_used)} of {fmt(summary.budget)}</span>
                <span className="text-slate-200 font-medium">{budgetPct.toFixed(0)}%</span>
              </div>
              <Progress value={budgetPct} />
            </>
          ) : <p className="text-sm text-slate-400">No budget set</p>}
          <div className="flex gap-2 mt-2">
            <input
              type="number" step="0.01" min="0" value={budgetInput} onChange={(e) => setBudgetInput(e.target.value)}
              placeholder="Set budget ($)"
              aria-label="Daily budget in USD"
              className="flex-1 px-3 py-2 text-sm bg-white/[0.03] border border-white/[0.08] rounded-lg text-slate-200 placeholder:text-slate-400/50 focus:outline-none focus:border-glow-cyan/40"
            />
            <Button size="sm" onClick={setBudget} disabled={saving}>{saving ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : "Set"}</Button>
          </div>
        </CardContent>
      </Card>

      {/* Breakdown by model */}
      {summary?.by_model && Object.keys(summary.by_model).length > 0 && (
        <Card>
          <CardHeader className="border-b border-white/[0.06]"><CardTitle className="text-sm">By Model</CardTitle></CardHeader>
          <CardContent className="p-4 space-y-2">
            {Object.entries(summary.by_model).sort((a, b) => b[1] - a[1]).map(([model, cost]) => (
              <div key={model} className="flex justify-between text-sm">
                <span className="text-slate-400 font-mono">{model}</span>
                <span className="text-slate-200 tabular-nums">{fmt(cost)}</span>
              </div>
            ))}
          </CardContent>
        </Card>
      )}

      {/* Breakdown by project */}
      {summary?.by_project && Object.keys(summary.by_project).length > 0 && (
        <Card>
          <CardHeader className="border-b border-white/[0.06]"><CardTitle className="text-sm">By Project</CardTitle></CardHeader>
          <CardContent className="p-4 space-y-2">
            {Object.entries(summary.by_project).sort((a, b) => b[1] - a[1]).map(([proj, cost]) => (
              <div key={proj} className="flex justify-between text-sm">
                <span className="text-slate-400 truncate mr-4">{proj}</span>
                <span className="text-slate-200 tabular-nums">{fmt(cost)}</span>
              </div>
            ))}
          </CardContent>
        </Card>
      )}

      {/* Recommendations */}
      {recommendations.length > 0 && (
        <Card>
          <CardHeader className="border-b border-white/[0.06]"><CardTitle className="text-sm flex items-center gap-2"><TrendingDown className="h-4 w-4" />Recommendations</CardTitle></CardHeader>
          <CardContent className="p-4 space-y-3">
            {recommendations.map((r, i) => (
              <div key={r.id ?? i} className="flex items-start gap-3 p-3 rounded-lg bg-white/[0.02]">
                <AlertTriangle className="h-4 w-4 text-amber-400 shrink-0 mt-0.5" />
                <div>
                  <p className="text-sm font-medium text-slate-200">{r.title ?? "Suggestion"}</p>
                  <p className="text-xs text-slate-400 mt-0.5">{r.description}</p>
                  {r.savings != null && <Badge variant="success" className="mt-1.5 text-[10px]">Save {fmt(r.savings)}</Badge>}
                </div>
              </div>
            ))}
          </CardContent>
        </Card>
      )}

      {/* Model route info */}
      {modelRoute && Object.keys(modelRoute).length > 0 && (
        <Card>
          <CardHeader className="border-b border-white/[0.06]"><CardTitle className="text-sm">Model Routing</CardTitle></CardHeader>
          <CardContent className="p-4">
            <pre className="text-xs text-slate-400 bg-white/[0.02] rounded-lg p-3 overflow-x-auto">{JSON.stringify(modelRoute, null, 2)}</pre>
          </CardContent>
        </Card>
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------
// 4. Control
// ---------------------------------------------------------------------------

interface ControlStatus { mode?: string; breakers?: Record<string, { tripped?: boolean; trip_count?: number }>; pending_approvals?: number }

function ControlPanel() {
  const { data, loading, reload } = useData(() => get<ControlStatus>("/control/status"));
  const [acting, setActing] = useState(false);

  async function setMode(mode: string) {
    setActing(true);
    try { await post("/control/mode", { mode }); reload(); } catch (e) { reportAdminError(e); }
    finally { setActing(false); }
  }

  async function tripBreaker(name: string) {
    setActing(true);
    try { await post("/control/breaker/trip", { name }); reload(); } catch (e) { reportAdminError(e); }
    finally { setActing(false); }
  }

  async function resetBreaker(name: string) {
    setActing(true);
    try { await post("/control/breaker/reset", { name }); reload(); } catch (e) { reportAdminError(e); }
    finally { setActing(false); }
  }

  if (loading) return <LoadingSkeleton rows={3} />;

  const modes = ["safe", "assisted", "autonomous"];
  const currentMode = data?.mode ?? "safe";

  return (
    <div className="space-y-4">
      {/* Mode selector */}
      <Card>
        <CardHeader className="border-b border-white/[0.06]"><CardTitle className="text-sm">Operation Mode</CardTitle></CardHeader>
        <CardContent className="p-4">
          <div className="flex gap-2">
            {modes.map((m) => (
              <Button
                key={m}
                variant={currentMode === m ? "default" : "outline"}
                size="sm"
                onClick={() => setMode(m)}
                disabled={acting}
                className="capitalize"
              >
                {m === "safe" && <Shield className="h-3.5 w-3.5 mr-1.5" />}
                {m === "assisted" && <Eye className="h-3.5 w-3.5 mr-1.5" />}
                {m === "autonomous" && <Zap className="h-3.5 w-3.5 mr-1.5" />}
                {m}
              </Button>
            ))}
          </div>
          {data?.pending_approvals != null && data.pending_approvals > 0 && (
            <div className="mt-3 flex items-center gap-2">
              <Badge variant="warning">{data.pending_approvals} pending approval{data.pending_approvals > 1 ? "s" : ""}</Badge>
            </div>
          )}
        </CardContent>
      </Card>

      {/* Circuit breakers */}
      <Card>
        <CardHeader className="border-b border-white/[0.06]"><CardTitle className="text-sm">Circuit Breakers</CardTitle></CardHeader>
        <CardContent className="p-4">
          {data?.breakers && Object.keys(data.breakers).length > 0 ? (
            <div className="space-y-3">
              {Object.entries(data.breakers).map(([name, b]) => (
                <div key={name} className="flex items-center justify-between py-2 px-3 rounded-lg bg-white/[0.02]">
                  <div className="flex items-center gap-3">
                    <StatusDot ok={!b.tripped} />
                    <div>
                      <p className="text-sm font-medium text-slate-200">{name}</p>
                      {b.trip_count != null && <p className="text-[10px] text-slate-400">Tripped {b.trip_count}x</p>}
                    </div>
                  </div>
                  <div className="flex gap-2">
                    {b.tripped ? (
                      <Button variant="outline" size="sm" onClick={() => resetBreaker(name)} disabled={acting}>
                        <RotateCcw className="h-3.5 w-3.5 mr-1.5" />Reset
                      </Button>
                    ) : (
                      <Button variant="destructive" size="sm" onClick={() => tripBreaker(name)} disabled={acting}>
                        <AlertTriangle className="h-3.5 w-3.5 mr-1.5" />Trip
                      </Button>
                    )}
                  </div>
                </div>
              ))}
            </div>
          ) : (
            <p className="text-sm text-slate-400 text-center py-4">No circuit breakers configured</p>
          )}
        </CardContent>
      </Card>
    </div>
  );
}

// ---------------------------------------------------------------------------
// 5. Super Agents
// ---------------------------------------------------------------------------

interface SuperAgent { id?: string; name: string; status?: string; metrics?: Record<string, unknown> }

function SuperAgentsPanel() {
  const { data: statusData, loading: l1, reload } = useData(() => get<Record<string, unknown>>("/super-agents/status"));
  const { data: agentsData, loading: l2 } = useData(() => get<{ agents?: SuperAgent[] }>("/super-agents/agents"));
  const { data: metricsData, loading: l3 } = useData(() => get<Record<string, unknown>>("/super-agents/metrics"));
  const { data: historyData } = useData(() => get<{ entries?: { timestamp?: string; event?: string; agent?: string }[] }>("/super-agents/history"));
  const [acting, setActing] = useState(false);

  const agents: SuperAgent[] = agentsData?.agents ?? (Array.isArray(agentsData) ? agentsData as SuperAgent[] : []);

  async function trigger(name: string) {
    setActing(true);
    try { await post("/super-agents/trigger", { agent: name }); reload(); } catch (e) { reportAdminError(e); }
    finally { setActing(false); }
  }

  async function pauseAgent(name: string) {
    setActing(true);
    try { await post("/super-agents/pause", { agent: name }); reload(); } catch (e) { reportAdminError(e); }
    finally { setActing(false); }
  }

  async function setMode(mode: string) {
    setActing(true);
    try { await post("/super-agents/mode", { mode }); reload(); } catch (e) { reportAdminError(e); }
    finally { setActing(false); }
  }

  if (l1 && l2 && l3) return <LoadingSkeleton rows={5} />;

  const history = historyData?.entries ?? (Array.isArray(historyData) ? historyData as { timestamp?: string; event?: string; agent?: string }[] : []);

  return (
    <div className="space-y-4">
      {/* Status overview */}
      {statusData && (
        <Card>
          <CardHeader className="border-b border-white/[0.06]">
            <div className="flex items-center justify-between">
              <CardTitle className="text-sm">System Status</CardTitle>
              <div className="flex gap-2">
                <Button variant="outline" size="sm" onClick={() => setMode("active")} disabled={acting}>Active</Button>
                <Button variant="outline" size="sm" onClick={() => setMode("passive")} disabled={acting}>Passive</Button>
              </div>
            </div>
          </CardHeader>
          <CardContent className="p-4">
            <pre className="text-xs text-slate-400 bg-white/[0.02] rounded-lg p-3 overflow-x-auto max-h-32">{JSON.stringify(statusData, null, 2)}</pre>
          </CardContent>
        </Card>
      )}

      {/* Agent list */}
      <Card>
        <CardHeader className="border-b border-white/[0.06]"><CardTitle className="text-sm">Agents ({agents.length})</CardTitle></CardHeader>
        <CardContent className="p-0 divide-y divide-white/[0.04]">
          {agents.length === 0 ? (
            <p className="text-sm text-slate-400 text-center py-8">No super agents registered</p>
          ) : agents.map((a, i) => {
            const running = a.status === "running" || a.status === "active";
            return (
              <div key={a.id ?? i} className="flex items-center justify-between px-4 py-3">
                <div className="flex items-center gap-3">
                  <StatusDot ok={running} />
                  <div>
                    <p className="text-sm font-medium text-slate-200">{a.name}</p>
                    <Badge variant={running ? "success" : "secondary"} className="text-[10px] mt-0.5">{a.status ?? "unknown"}</Badge>
                  </div>
                </div>
                <div className="flex gap-2">
                  <Button variant="outline" size="sm" onClick={() => trigger(a.name)} disabled={acting}>
                    <Play className="h-3 w-3 mr-1" />Trigger
                  </Button>
                  <Button variant="ghost" size="sm" onClick={() => pauseAgent(a.name)} disabled={acting}>
                    <Pause className="h-3 w-3 mr-1" />{running ? "Pause" : "Resume"}
                  </Button>
                </div>
              </div>
            );
          })}
        </CardContent>
      </Card>

      {/* Metrics */}
      {metricsData && Object.keys(metricsData).length > 0 && (
        <Card>
          <CardHeader className="border-b border-white/[0.06]"><CardTitle className="text-sm">Metrics</CardTitle></CardHeader>
          <CardContent className="p-4">
            <pre className="text-xs text-slate-400 bg-white/[0.02] rounded-lg p-3 overflow-x-auto max-h-48">{JSON.stringify(metricsData, null, 2)}</pre>
          </CardContent>
        </Card>
      )}

      {/* History */}
      {history.length > 0 && (
        <Card>
          <CardHeader className="border-b border-white/[0.06]"><CardTitle className="text-sm">Recent History</CardTitle></CardHeader>
          <CardContent className="p-0">
            <table className="w-full">
              <thead>
                <tr className="border-b border-white/[0.06]">
                  <th className="text-left px-4 py-2.5 text-[11px] font-medium text-slate-400">Time</th>
                  <th className="text-left px-4 py-2.5 text-[11px] font-medium text-slate-400">Agent</th>
                  <th className="text-left px-4 py-2.5 text-[11px] font-medium text-slate-400">Event</th>
                </tr>
              </thead>
              <tbody>
                {history.slice(0, 20).map((h, i) => (
                  <tr key={i} className="border-b border-white/[0.04] last:border-0">
                    <td className="px-4 py-2 text-xs text-slate-400 tabular-nums">{h.timestamp ? new Date(h.timestamp).toLocaleTimeString() : "--"}</td>
                    <td className="px-4 py-2 text-xs text-slate-200">{h.agent ?? "--"}</td>
                    <td className="px-4 py-2 text-xs text-slate-400">{h.event ?? "--"}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </CardContent>
        </Card>
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------
// 6. Eval
// ---------------------------------------------------------------------------

interface EvalSuite { id: string; name: string; description?: string; test_count?: number }
interface EvalResult { id?: string; suite_id?: string; suite_name?: string; passed?: number; failed?: number; total?: number; score?: number; timestamp?: string }

function EvalPanel() {
  const { data: suitesData, loading: l1 } = useData(() => get<{ suites?: EvalSuite[] }>("/eval/suites"));
  const { data: resultsData, loading: l2, reload: reloadResults } = useData(() => get<{ results?: EvalResult[] }>("/eval/results"));
  const [running, setRunning] = useState(false);
  const [selectedSuite, setSelectedSuite] = useState("");

  const suites: EvalSuite[] = suitesData?.suites ?? (Array.isArray(suitesData) ? suitesData as EvalSuite[] : []);
  const results: EvalResult[] = resultsData?.results ?? (Array.isArray(resultsData) ? resultsData as EvalResult[] : []);

  async function runEval() {
    setRunning(true);
    try {
      await post("/eval/run", selectedSuite ? { suite_id: selectedSuite } : {});
      reloadResults();
    } catch (e) { reportAdminError(e); }
    finally { setRunning(false); }
  }

  if (l1 && l2) return <LoadingSkeleton rows={4} />;

  return (
    <div className="space-y-4">
      {/* Run eval */}
      <Card>
        <CardHeader className="border-b border-white/[0.06]"><CardTitle className="text-sm">Run Evaluation</CardTitle></CardHeader>
        <CardContent className="p-4">
          <div className="flex gap-2">
            <select
              value={selectedSuite} onChange={(e) => setSelectedSuite(e.target.value)}
              className="flex-1 px-3 py-2 text-sm bg-white/[0.03] border border-white/[0.08] rounded-lg text-slate-200 focus:outline-none focus:border-glow-cyan/40"
            >
              <option value="">All suites</option>
              {suites.map((s) => <option key={s.id} value={s.id}>{s.name}</option>)}
            </select>
            <Button size="sm" onClick={runEval} disabled={running}>
              {running ? <Loader2 className="h-3.5 w-3.5 animate-spin mr-1.5" /> : <Play className="h-3.5 w-3.5 mr-1.5" />}
              Run
            </Button>
          </div>
        </CardContent>
      </Card>

      {/* Suites */}
      {suites.length > 0 && (
        <Card>
          <CardHeader className="border-b border-white/[0.06]"><CardTitle className="text-sm">Suites ({suites.length})</CardTitle></CardHeader>
          <CardContent className="p-0 divide-y divide-white/[0.04]">
            {suites.map((s) => (
              <div key={s.id} className="flex items-center justify-between px-4 py-3">
                <div>
                  <p className="text-sm font-medium text-slate-200">{s.name}</p>
                  {s.description && <p className="text-xs text-slate-400 mt-0.5">{s.description}</p>}
                </div>
                {s.test_count != null && <Badge variant="secondary">{s.test_count} tests</Badge>}
              </div>
            ))}
          </CardContent>
        </Card>
      )}

      {/* Results */}
      <Card>
        <CardHeader className="border-b border-white/[0.06]"><CardTitle className="text-sm">Results</CardTitle></CardHeader>
        <CardContent className="p-0">
          {results.length === 0 ? (
            <p className="text-sm text-slate-400 text-center py-8">No evaluation results yet</p>
          ) : (
            <table className="w-full">
              <thead>
                <tr className="border-b border-white/[0.06]">
                  <th className="text-left px-4 py-2.5 text-[11px] font-medium text-slate-400">Suite</th>
                  <th className="text-center px-4 py-2.5 text-[11px] font-medium text-slate-400">Result</th>
                  <th className="text-center px-4 py-2.5 text-[11px] font-medium text-slate-400">Score</th>
                  <th className="text-right px-4 py-2.5 text-[11px] font-medium text-slate-400">Time</th>
                </tr>
              </thead>
              <tbody>
                {results.map((r, i) => {
                  const allPassed = r.failed === 0;
                  return (
                    <tr key={r.id ?? i} className="border-b border-white/[0.04] last:border-0">
                      <td className="px-4 py-2 text-xs text-slate-200">{r.suite_name ?? r.suite_id ?? "--"}</td>
                      <td className="px-4 py-2 text-center">
                        <div className="flex items-center justify-center gap-2">
                          <Badge variant="success" className="text-[10px]">{r.passed ?? 0} pass</Badge>
                          {(r.failed ?? 0) > 0 && <Badge variant="destructive" className="text-[10px]">{r.failed} fail</Badge>}
                        </div>
                      </td>
                      <td className="px-4 py-2 text-center">
                        {r.score != null ? (
                          <span className={`text-sm font-bold ${allPassed ? "text-emerald-400" : "text-amber-400"}`}>{typeof r.score === "number" ? (r.score * 100).toFixed(0) + "%" : r.score}</span>
                        ) : "--"}
                      </td>
                      <td className="px-4 py-2 text-xs text-slate-400 text-right">{r.timestamp ? new Date(r.timestamp).toLocaleString() : "--"}</td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          )}
        </CardContent>
      </Card>
    </div>
  );
}

// ---------------------------------------------------------------------------
// 7. Security
// ---------------------------------------------------------------------------

interface ApiKey { id: string; name?: string; prefix?: string; created_at?: string; last_used?: string; scopes?: string[] }
interface AuditEntry { id?: string; action?: string; actor?: string; resource?: string; timestamp?: string; details?: string }

function SecurityPanel() {
  const { data: keysData, loading: l1, reload: reloadKeys } = useData(() => get<{ keys?: ApiKey[] }>("/auth/keys"));
  const { data: auditData, loading: l2 } = useData(() => get<{ entries?: AuditEntry[] }>("/audit/log"));
  const [newKeyName, setNewKeyName] = useState("");
  const [creating, setCreating] = useState(false);
  const [createdKey, setCreatedKey] = useState<string | null>(null);
  const [auditSearch, setAuditSearch] = useState("");

  const keys: ApiKey[] = keysData?.keys ?? (Array.isArray(keysData) ? keysData as ApiKey[] : []);
  const auditEntries: AuditEntry[] = auditData?.entries ?? (Array.isArray(auditData) ? auditData as AuditEntry[] : []);
  const filteredAudit = auditSearch
    ? auditEntries.filter((e) => e.action?.includes(auditSearch) || e.actor?.includes(auditSearch) || e.resource?.includes(auditSearch))
    : auditEntries;

  async function createKey() {
    if (!newKeyName.trim()) return;
    setCreating(true);
    try {
      const res = await post<{ key?: string; secret?: string }>("/auth/keys", { name: newKeyName.trim() });
      setCreatedKey(res.key ?? res.secret ?? null);
      setNewKeyName("");
      reloadKeys();
    } catch (e) { reportAdminError(e); }
    finally { setCreating(false); }
  }

  async function revokeKey(id: string) {
    try { await del(`/auth/keys/${id}`); reloadKeys(); } catch (e) { reportAdminError(e); }
  }

  return (
    <div className="space-y-4">
      {/* API Keys */}
      <Card>
        <CardHeader className="border-b border-white/[0.06]">
          <CardTitle className="text-sm">API Keys</CardTitle>
        </CardHeader>
        <CardContent className="p-4 space-y-3">
          <div className="flex gap-2">
            <input
              type="text" value={newKeyName} onChange={(e) => setNewKeyName(e.target.value)}
              placeholder="Key name..."
              className="flex-1 px-3 py-2 text-sm bg-white/[0.03] border border-white/[0.08] rounded-lg text-slate-200 placeholder:text-slate-400/50 focus:outline-none focus:border-glow-cyan/40"
              onKeyDown={(e) => e.key === "Enter" && createKey()}
            />
            <Button size="sm" onClick={createKey} disabled={creating}>
              {creating ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : <><Plus className="h-3.5 w-3.5 mr-1" />Create</>}
            </Button>
          </div>
          {createdKey && (
            <div className="p-3 rounded-lg bg-emerald-500/10 border border-emerald-500/20">
              <p className="text-xs text-emerald-400 mb-1">New key created (copy now, shown once):</p>
              <code className="text-sm text-emerald-300 font-mono break-all">{createdKey}</code>
            </div>
          )}
          {l1 ? <LoadingSkeleton rows={2} /> : keys.length === 0 ? (
            <p className="text-sm text-slate-400 text-center py-4">No API keys</p>
          ) : (
            <div className="space-y-2">
              {keys.map((k) => (
                <div key={k.id} className="flex items-center justify-between py-2 px-3 rounded-lg bg-white/[0.02]">
                  <div>
                    <p className="text-sm font-medium text-slate-200">{k.name ?? k.prefix ?? (k.id ?? "").slice(0, 8)}</p>
                    <div className="flex items-center gap-2 mt-0.5">
                      {k.prefix && <span className="text-[10px] text-slate-400 font-mono">{k.prefix}...</span>}
                      {k.created_at && <span className="text-[10px] text-slate-400">Created {new Date(k.created_at).toLocaleDateString()}</span>}
                    </div>
                  </div>
                  <Button variant="ghost" size="sm" onClick={() => revokeKey(k.id)} className="text-red-400 hover:text-red-300">
                    <Trash2 className="h-3.5 w-3.5" />
                  </Button>
                </div>
              ))}
            </div>
          )}
        </CardContent>
      </Card>

      {/* Audit log */}
      <Card>
        <CardHeader className="border-b border-white/[0.06]">
          <div className="flex items-center justify-between">
            <CardTitle className="text-sm">Audit Log</CardTitle>
            <div className="relative w-48">
              <Search className="absolute left-2.5 top-1/2 -translate-y-1/2 w-3.5 h-3.5 text-slate-400" />
              <input
                type="text" value={auditSearch} onChange={(e) => setAuditSearch(e.target.value)}
                placeholder="Search..."
                className="w-full pl-8 pr-3 py-1.5 text-xs bg-white/[0.03] border border-white/[0.08] rounded-lg text-slate-200 placeholder:text-slate-400/50 focus:outline-none focus:border-glow-cyan/40"
              />
            </div>
          </div>
        </CardHeader>
        <CardContent className="p-0">
          {l2 ? <div className="p-4"><LoadingSkeleton rows={3} /></div> : filteredAudit.length === 0 ? (
            <p className="text-sm text-slate-400 text-center py-8">No audit entries</p>
          ) : (
            <table className="w-full">
              <thead>
                <tr className="border-b border-white/[0.06]">
                  <th className="text-left px-4 py-2.5 text-[11px] font-medium text-slate-400">Time</th>
                  <th className="text-left px-4 py-2.5 text-[11px] font-medium text-slate-400">Action</th>
                  <th className="text-left px-4 py-2.5 text-[11px] font-medium text-slate-400">Actor</th>
                  <th className="text-left px-4 py-2.5 text-[11px] font-medium text-slate-400">Resource</th>
                </tr>
              </thead>
              <tbody>
                {filteredAudit.slice(0, 50).map((e, i) => (
                  <tr key={e.id ?? i} className="border-b border-white/[0.04] last:border-0 hover:bg-white/[0.02]">
                    <td className="px-4 py-2 text-xs text-slate-400 tabular-nums">{e.timestamp ? new Date(e.timestamp).toLocaleString() : "--"}</td>
                    <td className="px-4 py-2"><Badge variant="outline" className="text-[10px]">{e.action ?? "--"}</Badge></td>
                    <td className="px-4 py-2 text-xs text-slate-200">{e.actor ?? "--"}</td>
                    <td className="px-4 py-2 text-xs text-slate-400 font-mono truncate max-w-[200px]">{e.resource ?? "--"}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </CardContent>
      </Card>
    </div>
  );
}

// ---------------------------------------------------------------------------
// 8. MCP
// ---------------------------------------------------------------------------

interface McpServer { id: string; name?: string; url?: string; status?: string; connected?: boolean }
interface McpTool { name: string; description?: string; server_id?: string }

function McpPanel() {
  const { data: serversData, loading, reload } = useData(() => get<{ servers?: McpServer[] }>("/mcp/servers"));
  const { data: allTools } = useData(() => get<{ tools?: McpTool[] }>("/mcp/tools"));
  const [newName, setNewName] = useState("");
  const [newUrl, setNewUrl] = useState("");
  const [adding, setAdding] = useState(false);
  const [expandedServer, setExpandedServer] = useState<string | null>(null);
  const [serverTools, setServerTools] = useState<Record<string, McpTool[]>>({});

  const servers: McpServer[] = serversData?.servers ?? (Array.isArray(serversData) ? serversData as McpServer[] : []);

  async function addServer() {
    if (!newName.trim() || !newUrl.trim()) return;
    setAdding(true);
    try { await post("/mcp/servers", { name: newName.trim(), url: newUrl.trim() }); setNewName(""); setNewUrl(""); reload(); } catch (e) { reportAdminError(e); }
    finally { setAdding(false); }
  }

  async function removeServer(id: string) {
    try { await del(`/mcp/servers/${id}`); reload(); } catch (e) { reportAdminError(e); }
  }

  async function connectServer(id: string) {
    try { await post(`/mcp/servers/${id}/connect`); reload(); } catch (e) { reportAdminError(e); }
  }

  async function loadTools(id: string) {
    if (serverTools[id]) { setExpandedServer(expandedServer === id ? null : id); return; }
    try {
      const res = await get<{ tools?: McpTool[] }>(`/mcp/servers/${id}/tools`);
      const tools = res?.tools ?? (Array.isArray(res) ? res as McpTool[] : []);
      setServerTools((prev) => ({ ...prev, [id]: tools }));
      setExpandedServer(id);
    } catch { setExpandedServer(expandedServer === id ? null : id); }
  }

  return (
    <div className="space-y-4">
      {/* Add server */}
      <Card>
        <CardHeader className="border-b border-white/[0.06]"><CardTitle className="text-sm">Add MCP Server</CardTitle></CardHeader>
        <CardContent className="p-4">
          <div className="flex gap-2">
            <input type="text" value={newName} onChange={(e) => setNewName(e.target.value)} placeholder="Server name"
              className="flex-1 px-3 py-2 text-sm bg-white/[0.03] border border-white/[0.08] rounded-lg text-slate-200 placeholder:text-slate-400/50 focus:outline-none focus:border-glow-cyan/40" />
            <input type="text" value={newUrl} onChange={(e) => setNewUrl(e.target.value)} placeholder="URL (e.g. http://localhost:3000)"
              className="flex-1 px-3 py-2 text-sm bg-white/[0.03] border border-white/[0.08] rounded-lg text-slate-200 placeholder:text-slate-400/50 focus:outline-none focus:border-glow-cyan/40" />
            <Button size="sm" onClick={addServer} disabled={adding}>
              {adding ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : <><Plus className="h-3.5 w-3.5 mr-1" />Add</>}
            </Button>
          </div>
        </CardContent>
      </Card>

      {/* Server list */}
      <Card>
        <CardHeader className="border-b border-white/[0.06]"><CardTitle className="text-sm">Servers ({servers.length})</CardTitle></CardHeader>
        <CardContent className="p-0">
          {loading ? <div className="p-4"><LoadingSkeleton rows={3} /></div> : servers.length === 0 ? (
            <p className="text-sm text-slate-400 text-center py-8">No MCP servers registered</p>
          ) : (
            <div className="divide-y divide-white/[0.04]">
              {servers.map((s) => {
                const connected = s.connected || s.status === "connected";
                const isExpanded = expandedServer === s.id;
                return (
                  <div key={s.id}>
                    <div className="flex items-center justify-between px-4 py-3">
                      <div className="flex items-center gap-3">
                        <StatusDot ok={connected} />
                        <div>
                          <p className="text-sm font-medium text-slate-200">{s.name ?? s.id}</p>
                          {s.url && <p className="text-[10px] text-slate-400 font-mono">{s.url}</p>}
                        </div>
                      </div>
                      <div className="flex gap-2">
                        <Button variant="ghost" size="sm" onClick={() => loadTools(s.id)}>
                          {isExpanded ? <ChevronDown className="h-3.5 w-3.5" /> : <ChevronRight className="h-3.5 w-3.5" />}
                          <span className="ml-1">Tools</span>
                        </Button>
                        {!connected && (
                          <Button variant="outline" size="sm" onClick={() => connectServer(s.id)}>
                            <Link2 className="h-3.5 w-3.5 mr-1" />Connect
                          </Button>
                        )}
                        <Button variant="ghost" size="sm" onClick={() => removeServer(s.id)} className="text-red-400 hover:text-red-300">
                          <Trash2 className="h-3.5 w-3.5" />
                        </Button>
                      </div>
                    </div>
                    {isExpanded && (
                      <div className="px-4 pb-3 pl-10">
                        {serverTools[s.id]?.length ? (
                          <div className="space-y-1">
                            {serverTools[s.id].map((t, i) => (
                              <div key={i} className="flex items-start gap-2 py-1.5">
                                <CircleDot className="h-3 w-3 text-glow-cyan shrink-0 mt-0.5" />
                                <div>
                                  <p className="text-xs font-medium text-slate-200">{t.name}</p>
                                  {t.description && <p className="text-[10px] text-slate-400">{t.description}</p>}
                                </div>
                              </div>
                            ))}
                          </div>
                        ) : <p className="text-xs text-slate-400">No tools available</p>}
                      </div>
                    )}
                  </div>
                );
              })}
            </div>
          )}
        </CardContent>
      </Card>

      {/* All tools */}
      {allTools && (allTools.tools ?? []).length > 0 && (
        <Card>
          <CardHeader className="border-b border-white/[0.06]"><CardTitle className="text-sm">All Available Tools</CardTitle></CardHeader>
          <CardContent className="p-4">
            <div className="flex flex-wrap gap-2">
              {(allTools.tools ?? []).map((t, i) => (
                <Badge key={i} variant="outline" className="text-[10px]">{t.name}</Badge>
              ))}
            </div>
          </CardContent>
        </Card>
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------
// 9. Federation
// ---------------------------------------------------------------------------

interface Peer { id: string; name?: string; url?: string; trust_level?: string; status?: string; connected?: boolean }

function FederationPanel() {
  const { data: peersData, loading, reload } = useData(() => get<{ peers?: Peer[] }>("/kernel/federation/peers"));
  const [peerUrl, setPeerUrl] = useState("");
  const [connecting, setConnecting] = useState(false);

  const peers: Peer[] = peersData?.peers ?? (Array.isArray(peersData) ? peersData as Peer[] : []);

  async function connectPeer() {
    if (!peerUrl.trim()) return;
    setConnecting(true);
    try { await post("/kernel/federation/connect", { url: peerUrl.trim() }); setPeerUrl(""); reload(); } catch (e) { reportAdminError(e); }
    finally { setConnecting(false); }
  }

  async function disconnectPeer(id: string) {
    try { await del(`/kernel/federation/${id}`); reload(); } catch (e) { reportAdminError(e); }
  }

  const trustBadge = (level?: string) => {
    if (!level) return <Badge variant="secondary">unknown</Badge>;
    const map: Record<string, "success" | "warning" | "destructive" | "info"> = {
      full: "success", high: "success", medium: "warning", low: "destructive", trusted: "success", untrusted: "destructive",
    };
    return <Badge variant={map[level.toLowerCase()] ?? "info"}>{level}</Badge>;
  };

  return (
    <div className="space-y-4">
      {/* Connect form */}
      <Card>
        <CardHeader className="border-b border-white/[0.06]"><CardTitle className="text-sm">Connect to Peer</CardTitle></CardHeader>
        <CardContent className="p-4">
          <div className="flex gap-2">
            <input type="text" value={peerUrl} onChange={(e) => setPeerUrl(e.target.value)} placeholder="Peer URL (e.g. https://peer.example.com)"
              className="flex-1 px-3 py-2 text-sm bg-white/[0.03] border border-white/[0.08] rounded-lg text-slate-200 placeholder:text-slate-400/50 focus:outline-none focus:border-glow-cyan/40"
              onKeyDown={(e) => e.key === "Enter" && connectPeer()} />
            <Button size="sm" onClick={connectPeer} disabled={connecting}>
              {connecting ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : <><Globe className="h-3.5 w-3.5 mr-1" />Connect</>}
            </Button>
          </div>
        </CardContent>
      </Card>

      {/* Peers */}
      <Card>
        <CardHeader className="border-b border-white/[0.06]"><CardTitle className="text-sm">Connected Peers ({peers.length})</CardTitle></CardHeader>
        <CardContent className="p-0">
          {loading ? <div className="p-4"><LoadingSkeleton rows={3} /></div> : peers.length === 0 ? (
            <p className="text-sm text-slate-400 text-center py-8">No federation peers connected</p>
          ) : (
            <div className="divide-y divide-white/[0.04]">
              {peers.map((p) => {
                const isConnected = p.connected || p.status === "connected";
                return (
                  <div key={p.id} className="flex items-center justify-between px-4 py-3">
                    <div className="flex items-center gap-3">
                      <StatusDot ok={isConnected} />
                      <div>
                        <p className="text-sm font-medium text-slate-200">{p.name ?? p.id}</p>
                        {p.url && <p className="text-[10px] text-slate-400 font-mono">{p.url}</p>}
                      </div>
                    </div>
                    <div className="flex items-center gap-2">
                      {trustBadge(p.trust_level)}
                      <Button variant="ghost" size="sm" onClick={() => disconnectPeer(p.id)} className="text-red-400 hover:text-red-300">
                        <Unlink className="h-3.5 w-3.5 mr-1" />Disconnect
                      </Button>
                    </div>
                  </div>
                );
              })}
            </div>
          )}
        </CardContent>
      </Card>
    </div>
  );
}

// ---------------------------------------------------------------------------
// 10. Webhooks
// ---------------------------------------------------------------------------

interface WebhookEntry { id: string; url?: string; events?: string[]; active?: boolean; created_at?: string; last_delivery?: string; last_status?: number }

function WebhooksPanel() {
  const { data: whData, loading, reload } = useData(() => get<{ webhooks?: WebhookEntry[] }>("/webhooks"));
  const [newUrl, setNewUrl] = useState("");
  const [newEvents, setNewEvents] = useState("");
  const [adding, setAdding] = useState(false);
  const [testing, setTesting] = useState<string | null>(null);
  const [testResult, setTestResult] = useState<Record<string, { ok: boolean; message?: string }>>({});

  const webhooks: WebhookEntry[] = whData?.webhooks ?? (Array.isArray(whData) ? whData as WebhookEntry[] : []);

  async function addWebhook() {
    if (!newUrl.trim()) return;
    setAdding(true);
    try {
      const events = newEvents.trim() ? newEvents.split(",").map((e) => e.trim()).filter(Boolean) : undefined;
      await post("/webhooks", { url: newUrl.trim(), events });
      setNewUrl(""); setNewEvents(""); reload();
    } catch (e) { reportAdminError(e); }
    finally { setAdding(false); }
  }

  async function removeWebhook(id: string) {
    try { await del(`/webhooks/${id}`); reload(); } catch (e) { reportAdminError(e); }
  }

  async function testWebhook(id: string) {
    setTesting(id);
    try {
      const res = await post<{ success?: boolean; status?: number; message?: string }>(`/webhooks/${id}/test`);
      setTestResult((prev) => ({ ...prev, [id]: { ok: res.success !== false, message: res.message ?? `Status ${res.status ?? 200}` } }));
    } catch (e) {
      setTestResult((prev) => ({ ...prev, [id]: { ok: false, message: String(e) } }));
    }
    finally { setTesting(null); }
  }

  return (
    <div className="space-y-4">
      {/* Add webhook */}
      <Card>
        <CardHeader className="border-b border-white/[0.06]"><CardTitle className="text-sm">Register Webhook</CardTitle></CardHeader>
        <CardContent className="p-4 space-y-2">
          <div className="flex gap-2">
            <input type="text" value={newUrl} onChange={(e) => setNewUrl(e.target.value)} placeholder="Webhook URL"
              className="flex-1 px-3 py-2 text-sm bg-white/[0.03] border border-white/[0.08] rounded-lg text-slate-200 placeholder:text-slate-400/50 focus:outline-none focus:border-glow-cyan/40" />
            <Button size="sm" onClick={addWebhook} disabled={adding}>
              {adding ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : <><Plus className="h-3.5 w-3.5 mr-1" />Add</>}
            </Button>
          </div>
          <input type="text" value={newEvents} onChange={(e) => setNewEvents(e.target.value)} placeholder="Events (comma-separated, leave empty for all)"
            className="w-full px-3 py-2 text-sm bg-white/[0.03] border border-white/[0.08] rounded-lg text-slate-200 placeholder:text-slate-400/50 focus:outline-none focus:border-glow-cyan/40" />
        </CardContent>
      </Card>

      {/* Webhook list */}
      <Card>
        <CardHeader className="border-b border-white/[0.06]"><CardTitle className="text-sm">Registered Webhooks ({webhooks.length})</CardTitle></CardHeader>
        <CardContent className="p-0">
          {loading ? <div className="p-4"><LoadingSkeleton rows={3} /></div> : webhooks.length === 0 ? (
            <p className="text-sm text-slate-400 text-center py-8">No webhooks registered</p>
          ) : (
            <div className="divide-y divide-white/[0.04]">
              {webhooks.map((wh) => (
                <div key={wh.id} className="px-4 py-3 space-y-2">
                  <div className="flex items-center justify-between">
                    <div className="flex items-center gap-3">
                      <StatusDot ok={wh.active !== false} />
                      <div>
                        <p className="text-sm font-medium text-slate-200 font-mono">{wh.url ?? wh.id}</p>
                        <div className="flex items-center gap-2 mt-0.5">
                          {wh.events?.map((ev) => <Badge key={ev} variant="outline" className="text-[10px]">{ev}</Badge>)}
                          {wh.last_delivery && (
                            <span className="text-[10px] text-slate-400 flex items-center gap-1">
                              <Clock className="h-2.5 w-2.5" />Last: {new Date(wh.last_delivery).toLocaleString()}
                              {wh.last_status && <Badge variant={wh.last_status < 400 ? "success" : "destructive"} className="text-[10px] ml-1">{wh.last_status}</Badge>}
                            </span>
                          )}
                        </div>
                      </div>
                    </div>
                    <div className="flex gap-2">
                      <Button variant="outline" size="sm" onClick={() => testWebhook(wh.id)} disabled={testing === wh.id}>
                        {testing === wh.id ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : <><Send className="h-3.5 w-3.5 mr-1" />Test</>}
                      </Button>
                      <Button variant="ghost" size="sm" onClick={() => removeWebhook(wh.id)} className="text-red-400 hover:text-red-300">
                        <Trash2 className="h-3.5 w-3.5" />
                      </Button>
                    </div>
                  </div>
                  {testResult[wh.id] && (
                    <div className={`text-xs px-3 py-2 rounded-lg ${testResult[wh.id].ok ? "bg-emerald-500/10 text-emerald-400" : "bg-red-500/10 text-red-400"}`}>
                      {testResult[wh.id].ok ? <CheckCircle2 className="h-3 w-3 inline mr-1" /> : <XCircle className="h-3 w-3 inline mr-1" />}
                      {testResult[wh.id].message}
                    </div>
                  )}
                </div>
              ))}
            </div>
          )}
        </CardContent>
      </Card>
    </div>
  );
}
