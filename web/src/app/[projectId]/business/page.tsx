"use client";

import { useState, useEffect, useCallback } from "react";
import { useParams } from "next/navigation";
import {
  Building2,
  Users,
  Activity,
  DollarSign,
  AlertTriangle,
  Bot,
  Cpu,
  RefreshCw,
  ExternalLink,
  Plus,
  Clock,
  Zap,
  ShieldAlert,
  TrendingUp,
} from "lucide-react";
import { Card, CardHeader, CardTitle, CardContent, CardDescription } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { useToast } from "@/components/toast";
import { api } from "@/lib/api";
import { buildAppPreviewUrl } from "@/lib/utils";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface AppStatus {
  running: boolean;
  port?: number;
  url?: string;
}

interface TeamInstance {
  id: string;
  name: string;
  template: string;
  status: string;
  autonomy_level: string;
  created_at?: string;
}

interface TeamTemplate {
  name: string;
  description: string;
  roles: string[];
}

interface BusinessEvent {
  id: string;
  event_type: string;
  severity: string;
  team_id?: string | null;
  /** Backend name for this field. */
  created_at: string;
  /** Backend emits `payload` (object or string); we stringify for display. */
  payload?: unknown;
}

function eventMessage(event: BusinessEvent): string {
  if (typeof event.payload === "string") return event.payload;
  if (event.payload && typeof event.payload === "object") {
    const obj = event.payload as Record<string, unknown>;
    if (typeof obj.message === "string") return obj.message;
    return JSON.stringify(obj);
  }
  return event.event_type;
}

interface CostSummary {
  today_cost_usd: number;
  budget_limit_usd: number;
  percentage_used: number;
  total_tokens: number;
}

interface ProcessStats {
  running: number;
  total: number;
}

// ---------------------------------------------------------------------------
// Severity badge helper
// ---------------------------------------------------------------------------

function severityVariant(severity: string) {
  switch (severity) {
    case "critical":
      return "destructive" as const;
    case "warning":
      return "warning" as const;
    case "info":
      return "info" as const;
    default:
      return "secondary" as const;
  }
}

// ---------------------------------------------------------------------------
// Page
// ---------------------------------------------------------------------------

export default function BusinessPage() {
  const params = useParams();
  const projectId = params.projectId as string;
  const { toast } = useToast();

  // State
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [appStatus, setAppStatus] = useState<AppStatus>({ running: false });
  const [teams, setTeams] = useState<TeamInstance[]>([]);
  const [templates, setTemplates] = useState<TeamTemplate[]>([]);
  const [events, setEvents] = useState<BusinessEvent[]>([]);
  const [costs, setCosts] = useState<CostSummary | null>(null);
  const [processes, setProcesses] = useState<ProcessStats | null>(null);
  const [creatingTeam, setCreatingTeam] = useState<string | null>(null);
  const [refreshing, setRefreshing] = useState(false);

  // -------------------------------------------------------------------------
  // Fetch all data
  // -------------------------------------------------------------------------

  const fetchData = useCallback(async () => {
    try {
      const [overview, teamsData, eventsData] = await Promise.allSettled([
        api.getBusinessOverview(projectId),
        api.listBusinessTeams(projectId),
        api.listBusinessEvents(projectId),
      ]);

      // Overview
      if (overview.status === "fulfilled" && overview.value) {
        const ov = overview.value as Record<string, unknown>;
        setAppStatus({
          running: (ov.app_status as Record<string, unknown>)?.running === true,
          port: (ov.app_status as Record<string, unknown>)?.port as number | undefined,
          url: (ov.app_status as Record<string, unknown>)?.url as string | undefined,
        });
        if (ov.costs) {
          const c = ov.costs as Record<string, unknown>;
          setCosts({
            today_cost_usd: (c.today_cost_usd as number) ?? 0,
            budget_limit_usd: (c.budget_limit_usd as number) ?? 0,
            percentage_used: (c.percentage_used as number) ?? 0,
            total_tokens: (c.total_tokens as number) ?? 0,
          });
        }
        if (ov.processes) {
          const p = ov.processes as Record<string, unknown>;
          setProcesses({
            running: (p.running as number) ?? 0,
            total: (p.total as number) ?? 0,
          });
        }
      }

      // Teams
      if (teamsData.status === "fulfilled" && teamsData.value) {
        const td = teamsData.value as Record<string, unknown>;
        setTeams((td.instances as TeamInstance[]) ?? []);
        setTemplates((td.templates as TeamTemplate[]) ?? []);
      }

      // Events
      if (eventsData.status === "fulfilled" && eventsData.value) {
        const ed = eventsData.value as Record<string, unknown>;
        setEvents((ed.events as BusinessEvent[]) ?? []);
      }

      setError(null);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to load dashboard data");
    } finally {
      setLoading(false);
      setRefreshing(false);
    }
  }, [projectId]);

  // Initial load + polling
  useEffect(() => {
    fetchData();
    const interval = setInterval(fetchData, 30_000);
    return () => clearInterval(interval);
  }, [fetchData]);

  // -------------------------------------------------------------------------
  // Create team
  // -------------------------------------------------------------------------

  async function handleCreateTeam(templateName: string) {
    setCreatingTeam(templateName);
    try {
      await api.createBusinessTeam(projectId, templateName);
      toast("success", "Team created", templateName);
      await fetchData();
    } catch (err) {
      const msg = err instanceof Error ? err.message : "Failed to create team";
      toast("error", "Could not create team", msg);
    } finally {
      setCreatingTeam(null);
    }
  }

  // -------------------------------------------------------------------------
  // Manual refresh
  // -------------------------------------------------------------------------

  function handleRefresh() {
    setRefreshing(true);
    fetchData();
  }

  // -------------------------------------------------------------------------
  // Derived data
  // -------------------------------------------------------------------------

  const attentionItems = events.filter(
    (e) => e.severity === "critical" || e.severity === "warning",
  );

  // -------------------------------------------------------------------------
  // Render
  // -------------------------------------------------------------------------

  if (loading) {
    return (
      <div className="p-6 space-y-6">
        <div className="flex items-center gap-3">
          <Skeleton className="h-8 w-48" />
          <Skeleton className="h-8 w-24" />
        </div>
        <div className="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 gap-4">
          {Array.from({ length: 6 }).map((_, i) => (
            <Skeleton key={i} className="h-40 rounded-xl" />
          ))}
        </div>
      </div>
    );
  }

  if (error) {
    return (
      <div className="p-6 flex flex-col items-center justify-center gap-4 text-center">
        <AlertTriangle className="w-10 h-10 text-amber-400" />
        <p className="text-slate-300 text-sm max-w-md">{error}</p>
        <Button variant="outline" size="sm" onClick={handleRefresh}>
          <RefreshCw className="w-3.5 h-3.5 mr-1.5" />
          Retry
        </Button>
      </div>
    );
  }

  return (
    <div className="p-6 space-y-6 overflow-auto h-full">
      {/* Page header */}
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-3">
          <div className="w-9 h-9 rounded-lg bg-gradient-to-br from-purple-500/20 to-blue-500/20 border border-purple-500/10 flex items-center justify-center">
            <Building2 className="w-4 h-4 text-purple-400" />
          </div>
          <div>
            <h1 className="text-lg font-semibold text-slate-200">CEO Dashboard</h1>
            <p className="text-xs text-slate-400">Business overview and team management</p>
          </div>
        </div>
        <Button
          variant="outline"
          size="sm"
          onClick={handleRefresh}
          disabled={refreshing}
        >
          <RefreshCw className={`w-3.5 h-3.5 mr-1.5 ${refreshing ? "animate-spin" : ""}`} />
          Refresh
        </Button>
      </div>

      {/* Top row — status cards */}
      <div className="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-4 gap-4">
        {/* App Status */}
        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-sm font-medium text-slate-400 flex items-center gap-2">
              <Zap className="w-4 h-4" />
              App Status
            </CardTitle>
          </CardHeader>
          <CardContent>
            <div className="flex items-center gap-2 mb-2">
              <span
                className={`w-2 h-2 rounded-full ${
                  appStatus.running ? "bg-emerald-400 shadow-[0_0_6px_rgba(52,211,153,0.5)]" : "bg-slate-500"
                }`}
              />
              <span className="text-sm font-medium text-slate-200">
                {appStatus.running ? "Running" : "Stopped"}
              </span>
            </div>
            {appStatus.port && (
              <p className="text-xs text-slate-400 mb-1">Port: {appStatus.port}</p>
            )}
            {appStatus.running && appStatus.port && (
              <a
                href={buildAppPreviewUrl(appStatus.port, appStatus.url) ?? "#"}
                target="_blank"
                rel="noopener noreferrer"
                className="inline-flex items-center gap-1 text-xs text-glow-cyan hover:underline"
              >
                <ExternalLink className="w-3 h-3" />
                Open App
              </a>
            )}
          </CardContent>
        </Card>

        {/* Cost Summary */}
        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-sm font-medium text-slate-400 flex items-center gap-2">
              <DollarSign className="w-4 h-4" />
              Cost Summary
            </CardTitle>
          </CardHeader>
          <CardContent>
            {costs ? (
              <>
                <p className="text-lg font-semibold text-slate-200">
                  ${costs.today_cost_usd.toFixed(4)}
                </p>
                <div className="mt-1.5 w-full h-1.5 rounded-full bg-white/5">
                  <div
                    className={`h-full rounded-full transition-all ${
                      costs.percentage_used > 80
                        ? "bg-red-500"
                        : costs.percentage_used > 50
                          ? "bg-amber-500"
                          : "bg-emerald-500"
                    }`}
                    style={{ width: `${Math.min(costs.percentage_used, 100)}%` }}
                  />
                </div>
                <p className="text-xs text-slate-400 mt-1">
                  {costs.percentage_used.toFixed(1)}% of ${costs.budget_limit_usd.toFixed(2)} budget
                </p>
                <p className="text-xs text-slate-500 mt-0.5">
                  {costs.total_tokens.toLocaleString()} tokens today
                </p>
              </>
            ) : (
              <p className="text-xs text-slate-500">No cost data</p>
            )}
          </CardContent>
        </Card>

        {/* Process Stats */}
        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-sm font-medium text-slate-400 flex items-center gap-2">
              <Cpu className="w-4 h-4" />
              Processes
            </CardTitle>
          </CardHeader>
          <CardContent>
            {processes ? (
              <>
                <p className="text-lg font-semibold text-slate-200">
                  {processes.running}
                  <span className="text-sm font-normal text-slate-400">
                    {" "}/ {processes.total}
                  </span>
                </p>
                <p className="text-xs text-slate-400 mt-1">Running kernel processes</p>
              </>
            ) : (
              <p className="text-xs text-slate-500">No process data</p>
            )}
          </CardContent>
        </Card>

        {/* Attention Items */}
        <Card className={attentionItems.length > 0 ? "border-amber-500/20" : ""}>
          <CardHeader className="pb-2">
            <CardTitle className="text-sm font-medium text-slate-400 flex items-center gap-2">
              <ShieldAlert className="w-4 h-4" />
              Attention
            </CardTitle>
          </CardHeader>
          <CardContent>
            {attentionItems.length > 0 ? (
              <div className="space-y-1.5">
                {attentionItems.slice(0, 3).map((item) => (
                  <div key={item.id} className="flex items-start gap-2">
                    <AlertTriangle
                      className={`w-3 h-3 mt-0.5 flex-shrink-0 ${
                        item.severity === "critical" ? "text-red-400" : "text-amber-400"
                      }`}
                    />
                    <p className="text-xs text-slate-300 line-clamp-1">{eventMessage(item)}</p>
                  </div>
                ))}
                {attentionItems.length > 3 && (
                  <p className="text-xs text-slate-500">
                    +{attentionItems.length - 3} more
                  </p>
                )}
              </div>
            ) : (
              <div className="flex items-center gap-2">
                <span className="w-2 h-2 rounded-full bg-emerald-400" />
                <p className="text-xs text-slate-400">All clear</p>
              </div>
            )}
          </CardContent>
        </Card>
      </div>

      {/* Active Teams Grid */}
      <section>
        <h2 className="text-sm font-semibold text-slate-300 mb-3 flex items-center gap-2">
          <Users className="w-4 h-4 text-slate-400" />
          Active Teams
        </h2>
        {teams.length > 0 ? (
          <div className="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 gap-4">
            {teams.map((team) => (
              <Card key={team.id}>
                <CardHeader className="pb-2">
                  <div className="flex items-center justify-between">
                    <CardTitle className="text-sm font-medium text-slate-200 flex items-center gap-2">
                      <Bot className="w-4 h-4 text-glow-cyan" />
                      {team.name}
                    </CardTitle>
                    <Badge
                      variant={
                        team.status === "active"
                          ? "success"
                          : team.status === "paused"
                            ? "warning"
                            : "secondary"
                      }
                      className="text-[10px]"
                    >
                      {team.status}
                    </Badge>
                  </div>
                  <CardDescription className="text-xs">{team.template}</CardDescription>
                </CardHeader>
                <CardContent>
                  <div className="flex items-center gap-2">
                    <Badge variant="outline" className="text-[10px]">
                      {team.autonomy_level}
                    </Badge>
                    {team.created_at && (
                      <span className="text-[10px] text-slate-500">
                        Created {new Date(team.created_at).toLocaleDateString()}
                      </span>
                    )}
                  </div>
                </CardContent>
              </Card>
            ))}
          </div>
        ) : (
          <Card>
            <CardContent className="py-8 text-center">
              <Users className="w-8 h-8 text-slate-600 mx-auto mb-2" />
              <p className="text-sm text-slate-400">No active teams yet</p>
              <p className="text-xs text-slate-500 mt-1">
                Create a team from the templates below to get started
              </p>
            </CardContent>
          </Card>
        )}
      </section>

      {/* Create Team Section */}
      {templates.length > 0 && (
        <section>
          <h2 className="text-sm font-semibold text-slate-300 mb-3 flex items-center gap-2">
            <Plus className="w-4 h-4 text-slate-400" />
            Create a Team
          </h2>
          <div className="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 gap-4">
            {templates.map((tpl) => (
              <Card key={tpl.name}>
                <CardHeader className="pb-2">
                  <CardTitle className="text-sm font-medium text-slate-200">
                    {tpl.name}
                  </CardTitle>
                  <CardDescription className="text-xs line-clamp-2">
                    {tpl.description}
                  </CardDescription>
                </CardHeader>
                <CardContent>
                  <div className="flex flex-wrap gap-1 mb-3">
                    {tpl.roles.slice(0, 4).map((role) => (
                      <Badge key={role} variant="secondary" className="text-[10px]">
                        {role}
                      </Badge>
                    ))}
                    {tpl.roles.length > 4 && (
                      <Badge variant="outline" className="text-[10px]">
                        +{tpl.roles.length - 4}
                      </Badge>
                    )}
                  </div>
                  <Button
                    size="sm"
                    variant="outline"
                    onClick={() => handleCreateTeam(tpl.name)}
                    disabled={creatingTeam === tpl.name}
                    className="w-full"
                  >
                    {creatingTeam === tpl.name ? (
                      <RefreshCw className="w-3.5 h-3.5 mr-1.5 animate-spin" />
                    ) : (
                      <Plus className="w-3.5 h-3.5 mr-1.5" />
                    )}
                    Create
                  </Button>
                </CardContent>
              </Card>
            ))}
          </div>
        </section>
      )}

      {/* Recent Events Timeline */}
      <section>
        <h2 className="text-sm font-semibold text-slate-300 mb-3 flex items-center gap-2">
          <Activity className="w-4 h-4 text-slate-400" />
          Recent Events
        </h2>
        <Card>
          <CardContent className="py-3">
            {events.length > 0 ? (
              <div className="max-h-64 overflow-y-auto space-y-2 pr-1">
                {events.map((event) => (
                  <div
                    key={event.id}
                    className="flex items-start gap-3 py-2 border-b border-white/[0.04] last:border-0"
                  >
                    <div className="mt-0.5">
                      {event.severity === "critical" ? (
                        <AlertTriangle className="w-3.5 h-3.5 text-red-400" />
                      ) : event.severity === "warning" ? (
                        <AlertTriangle className="w-3.5 h-3.5 text-amber-400" />
                      ) : (
                        <TrendingUp className="w-3.5 h-3.5 text-slate-500" />
                      )}
                    </div>
                    <div className="flex-1 min-w-0">
                      <div className="flex items-center gap-2">
                        <Badge
                          variant={severityVariant(event.severity)}
                          className="text-[10px]"
                        >
                          {event.severity}
                        </Badge>
                        <span className="text-[10px] text-slate-500 truncate">
                          {event.event_type}
                        </span>
                      </div>
                      <p className="text-xs text-slate-300 mt-0.5 line-clamp-2">
                        {eventMessage(event)}
                      </p>
                    </div>
                    <span className="text-[10px] text-slate-500 whitespace-nowrap flex items-center gap-1">
                      <Clock className="w-2.5 h-2.5" />
                      {new Date(event.created_at).toLocaleTimeString()}
                    </span>
                  </div>
                ))}
              </div>
            ) : (
              <div className="py-6 text-center">
                <Activity className="w-6 h-6 text-slate-600 mx-auto mb-1.5" />
                <p className="text-xs text-slate-500">No events yet</p>
              </div>
            )}
          </CardContent>
        </Card>
      </section>
    </div>
  );
}
