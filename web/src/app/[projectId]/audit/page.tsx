"use client";

import { useState, useEffect, useCallback } from "react";
import { useParams } from "next/navigation";
import {
  Search,
  Shield,
  Clock,
  RefreshCw,
  CheckCircle2,
  XCircle,
  User,
  Bot,
  Cpu,
  Zap,
  ShieldCheck,
  Rocket,
  FileCode2,
  Filter,
  BarChart3,
  ChevronDown,
  ChevronUp,
} from "lucide-react";
import { cn } from "@/lib/utils";
import { BASE } from "@/lib/api";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { StaggerItem } from "@/components/ui/stagger-list";

/* ------------------------------------------------------------------ */
/*  Types                                                              */
/* ------------------------------------------------------------------ */

interface AuditEntry {
  id: string;
  project_id: string;
  actor: string;
  action: string;
  target?: string | null;
  details?: string | null;
  risk_level: string;
  created_at: string;
}

/* ------------------------------------------------------------------ */
/*  Constants                                                          */
/* ------------------------------------------------------------------ */

const ACTION_CATEGORIES = [
  { value: "all", label: "All Events" },
  { value: "generation", label: "Generation" },
  { value: "agent", label: "Agent" },
  { value: "team", label: "Team" },
  { value: "quality", label: "Quality" },
  { value: "approval", label: "Approval" },
  { value: "deploy", label: "Deploy" },
] as const;

type ActionFilter = (typeof ACTION_CATEGORIES)[number]["value"];

const RISK_STYLES: Record<string, { bg: string; text: string; border: string }> = {
  low: { bg: "bg-emerald-500/10", text: "text-emerald-400", border: "border-emerald-500/20" },
  medium: { bg: "bg-amber-500/10", text: "text-amber-400", border: "border-amber-500/20" },
  high: { bg: "bg-red-500/10", text: "text-red-400", border: "border-red-500/20" },
  critical: { bg: "bg-red-500/20", text: "text-red-300", border: "border-red-500/30" },
};

function getActionIcon(action: string | undefined | null) {
  const a = (action ?? "").toLowerCase();
  if (a.includes("generat") || a.includes("codegen") || a.includes("oneshot"))
    return FileCode2;
  if (a.includes("agent") || a.includes("coding")) return Bot;
  if (a.includes("team") || a.includes("wave")) return Cpu;
  if (a.includes("quality") || a.includes("taste") || a.includes("gate"))
    return ShieldCheck;
  if (a.includes("approv")) return CheckCircle2;
  if (a.includes("deploy") || a.includes("publish") || a.includes("run"))
    return Rocket;
  if (a.includes("error") || a.includes("fail")) return XCircle;
  return Zap;
}

function getActionCategory(action: string | undefined | null): string {
  const a = (action ?? "").toLowerCase();
  if (a.includes("generat") || a.includes("codegen") || a.includes("oneshot"))
    return "generation";
  if (a.includes("agent") || a.includes("coding")) return "agent";
  if (a.includes("team") || a.includes("wave")) return "team";
  if (a.includes("quality") || a.includes("taste") || a.includes("gate"))
    return "quality";
  if (a.includes("approv")) return "approval";
  if (a.includes("deploy") || a.includes("publish") || a.includes("run"))
    return "deploy";
  return "other";
}

function getActorIcon(actor: string | undefined | null) {
  const a = (actor ?? "").toLowerCase();
  if (a === "system" || a === "nexus") return Cpu;
  if (a.includes("agent") || a.includes("nova") || a.includes("atlas") || a.includes("kai"))
    return Bot;
  return User;
}

/* ------------------------------------------------------------------ */
/*  AuditEventRow                                                      */
/* ------------------------------------------------------------------ */

function AuditEventRow({ entry, index }: { entry: AuditEntry; index: number }) {
  const [expanded, setExpanded] = useState(false);
  const ActionIcon = getActionIcon(entry.action);
  const ActorIcon = getActorIcon(entry.actor);
  const riskStyle = RISK_STYLES[entry.risk_level] ?? RISK_STYLES.low;

  return (
    <StaggerItem index={index}>
      <div
        className={cn(
          "rounded-lg border bg-white/[0.02] hover:bg-white/[0.04] transition-colors",
          riskStyle.border
        )}
      >
        <button
          onClick={() => setExpanded(!expanded)}
          className="w-full p-4 text-left"
        >
          <div className="flex items-start gap-3">
            {/* Action icon */}
            <div className={cn("rounded-lg p-2 shrink-0", riskStyle.bg)}>
              <ActionIcon className={cn("h-4 w-4", riskStyle.text)} />
            </div>

            {/* Content */}
            <div className="min-w-0 flex-1">
              <div className="flex items-center gap-2 mb-1 flex-wrap">
                <span className="text-sm font-medium text-slate-200">
                  {entry.action}
                </span>
                <Badge variant="outline" className={cn("text-[10px]", riskStyle.text)}>
                  {entry.risk_level}
                </Badge>
                {entry.target && (
                  <span className="text-[11px] text-slate-500 truncate max-w-[200px]">
                    {entry.target}
                  </span>
                )}
              </div>
              <div className="flex items-center gap-3 text-[10px] text-slate-500">
                <span className="flex items-center gap-1">
                  <ActorIcon className="h-2.5 w-2.5" />
                  {entry.actor}
                </span>
                <span className="flex items-center gap-1">
                  <Clock className="h-2.5 w-2.5" />
                  {new Date(entry.created_at).toLocaleString()}
                </span>
              </div>
            </div>

            {/* Expand indicator */}
            {entry.details && (
              <div className="shrink-0">
                {expanded ? (
                  <ChevronUp className="h-4 w-4 text-slate-500" />
                ) : (
                  <ChevronDown className="h-4 w-4 text-slate-500" />
                )}
              </div>
            )}
          </div>
        </button>

        {/* Expanded details */}
        {expanded && entry.details && (
          <div className="px-4 pb-4 pt-0">
            <div className="rounded-lg bg-white/[0.03] border border-white/[0.06] p-3">
              <p className="text-[11px] text-slate-400 leading-relaxed whitespace-pre-wrap font-mono">
                {entry.details}
              </p>
            </div>
          </div>
        )}
      </div>
    </StaggerItem>
  );
}

/* ------------------------------------------------------------------ */
/*  Statistics Panel                                                   */
/* ------------------------------------------------------------------ */

function AuditStats({ entries }: { entries: AuditEntry[] }) {
  // Events by day (last 7 days)
  const dayCounts: Record<string, number> = {};
  const now = new Date();
  for (let i = 6; i >= 0; i--) {
    const d = new Date(now);
    d.setDate(d.getDate() - i);
    dayCounts[d.toISOString().slice(0, 10)] = 0;
  }
  for (const e of entries) {
    const day = typeof e.created_at === "string" ? e.created_at.slice(0, 10) : "";
    if (day && day in dayCounts) dayCounts[day]++;
  }
  const maxDayCount = Math.max(1, ...Object.values(dayCounts));

  // Most active actors
  const actorCounts: Record<string, number> = {};
  for (const e of entries) {
    const actor = e.actor ?? "unknown";
    actorCounts[actor] = (actorCounts[actor] ?? 0) + 1;
  }
  const topActors = Object.entries(actorCounts)
    .sort((a, b) => b[1] - a[1])
    .slice(0, 5);
  const maxActorCount = Math.max(1, topActors[0]?.[1] ?? 1);

  // Risk distribution
  const riskCounts: Record<string, number> = {};
  for (const e of entries) {
    riskCounts[e.risk_level] = (riskCounts[e.risk_level] ?? 0) + 1;
  }

  return (
    <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
      {/* Events per day */}
      <Card className="border-white/[0.08] bg-white/[0.02]">
        <CardHeader className="pb-2">
          <CardTitle className="text-xs font-medium text-slate-400 flex items-center gap-1.5">
            <BarChart3 className="h-3.5 w-3.5" />
            Events (Last 7 Days)
          </CardTitle>
        </CardHeader>
        <CardContent>
          <div className="flex items-end gap-1 h-20">
            {Object.entries(dayCounts).map(([day, count]) => (
              <div key={day} className="flex-1 flex flex-col items-center gap-1">
                <div
                  className="w-full rounded-sm bg-cyan-500/20 transition-all hover:bg-cyan-500/30"
                  style={{
                    height: `${Math.max(2, (count / maxDayCount) * 100)}%`,
                    minHeight: "2px",
                  }}
                />
                <span className="text-[8px] text-slate-600">
                  {new Date(day).toLocaleDateString(undefined, { weekday: "narrow" })}
                </span>
              </div>
            ))}
          </div>
        </CardContent>
      </Card>

      {/* Most active actors */}
      <Card className="border-white/[0.08] bg-white/[0.02]">
        <CardHeader className="pb-2">
          <CardTitle className="text-xs font-medium text-slate-400 flex items-center gap-1.5">
            <User className="h-3.5 w-3.5" />
            Most Active Actors
          </CardTitle>
        </CardHeader>
        <CardContent className="space-y-1.5">
          {topActors.length === 0 ? (
            <p className="text-[11px] text-slate-500">No data</p>
          ) : (
            topActors.map(([actor, count]) => (
              <div key={actor} className="flex items-center gap-2">
                <span className="text-[11px] text-slate-300 w-20 truncate">{actor}</span>
                <div className="flex-1 h-1.5 rounded-full bg-white/[0.05]">
                  <div
                    className="h-full rounded-full bg-violet-500/40"
                    style={{ width: `${(count / maxActorCount) * 100}%` }}
                  />
                </div>
                <span className="text-[10px] text-slate-500 w-6 text-right">{count}</span>
              </div>
            ))
          )}
        </CardContent>
      </Card>

      {/* Risk distribution */}
      <Card className="border-white/[0.08] bg-white/[0.02]">
        <CardHeader className="pb-2">
          <CardTitle className="text-xs font-medium text-slate-400 flex items-center gap-1.5">
            <Shield className="h-3.5 w-3.5" />
            Risk Distribution
          </CardTitle>
        </CardHeader>
        <CardContent className="space-y-1.5">
          {Object.keys(RISK_STYLES).map((level) => {
            const count = riskCounts[level] ?? 0;
            const style = RISK_STYLES[level];
            return (
              <div key={level} className="flex items-center gap-2">
                <span className={cn("text-[11px] w-14 capitalize", style.text)}>
                  {level}
                </span>
                <div className="flex-1 h-1.5 rounded-full bg-white/[0.05]">
                  <div
                    className={cn("h-full rounded-full", style.bg)}
                    style={{
                      width: entries.length > 0 ? `${(count / entries.length) * 100}%` : "0%",
                    }}
                  />
                </div>
                <span className="text-[10px] text-slate-500 w-6 text-right">{count}</span>
              </div>
            );
          })}
        </CardContent>
      </Card>
    </div>
  );
}

/* ------------------------------------------------------------------ */
/*  Page                                                               */
/* ------------------------------------------------------------------ */

const PAGE_SIZE = 50;

export default function AuditPage() {
  const params = useParams();
  const projectId = params.projectId as string;

  const [entries, setEntries] = useState<AuditEntry[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [searchQuery, setSearchQuery] = useState("");
  const [actionFilter, setActionFilter] = useState<ActionFilter>("all");
  const [showStats, setShowStats] = useState(true);
  const [visibleCount, setVisibleCount] = useState(PAGE_SIZE);

  const fetchAudit = useCallback(async () => {
    try {
      const res = await fetch(
        `${BASE}/projects/${projectId}/audit?limit=500`
      );
      if (!res.ok) throw new Error("Failed to load audit log");
      const data = await res.json();
      setEntries(Array.isArray(data) ? data : []);
      setError(null);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to load audit log");
    } finally {
      setLoading(false);
    }
  }, [projectId]);

  useEffect(() => {
    fetchAudit();
  }, [fetchAudit]);

  const handleRefresh = () => {
    setLoading(true);
    setVisibleCount(PAGE_SIZE);
    fetchAudit();
  };

  // Filter and search
  const filteredEntries = entries.filter((entry) => {
    const q = searchQuery.trim().toLowerCase();
    const matchesAction =
      actionFilter === "all" || getActionCategory(entry.action ?? "") === actionFilter;
    const matchesSearch =
      !q ||
      (entry.action ?? "").toLowerCase().includes(q) ||
      (entry.actor ?? "").toLowerCase().includes(q) ||
      (entry.target ?? "").toLowerCase().includes(q) ||
      (entry.details ?? "").toLowerCase().includes(q);
    return matchesAction && matchesSearch;
  });

  const visibleEntries = filteredEntries.slice(0, visibleCount);
  const hasMore = visibleCount < filteredEntries.length;

  return (
    <div className="flex flex-col gap-6 p-6 overflow-auto h-full">
      {/* Header */}
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-3">
          <div className="w-9 h-9 rounded-lg bg-gradient-to-br from-violet-500/20 to-purple-500/20 border border-violet-500/10 flex items-center justify-center">
            <Shield className="w-4 h-4 text-violet-400" />
          </div>
          <div>
            <h1 className="text-lg font-semibold text-slate-200">Audit Log</h1>
            <p className="text-xs text-slate-400">
              {loading
                ? "Loading..."
                : `${entries.length} event${entries.length !== 1 ? "s" : ""} recorded`}
            </p>
          </div>
        </div>
        <div className="flex items-center gap-2">
          <Button
            variant="ghost"
            size="sm"
            className={cn(
              "text-xs",
              showStats ? "text-cyan-400" : "text-slate-400 hover:text-slate-200"
            )}
            onClick={() => setShowStats(!showStats)}
          >
            <BarChart3 className="h-3.5 w-3.5 mr-1.5" />
            Stats
          </Button>
          <Button
            variant="ghost"
            size="sm"
            className="text-slate-400 hover:text-slate-200"
            onClick={handleRefresh}
          >
            <RefreshCw
              className={cn("h-4 w-4 mr-1.5", loading ? "animate-spin" : "")}
            />
            Refresh
          </Button>
        </div>
      </div>

      {/* Statistics */}
      {showStats && !loading && entries.length > 0 && (
        <AuditStats entries={entries} />
      )}

      {/* Search and filter bar */}
      <div className="flex items-center gap-3 flex-wrap">
        <div className="relative flex-1 min-w-[200px]">
          <Search className="absolute left-3 top-1/2 -translate-y-1/2 h-4 w-4 text-slate-500" />
          <input
            type="text"
            value={searchQuery}
            onChange={(e) => {
              setSearchQuery(e.target.value);
              setVisibleCount(PAGE_SIZE);
            }}
            placeholder="Search events..."
            aria-label="Search audit events"
            className="w-full rounded-lg border border-white/[0.08] bg-white/[0.03] pl-10 pr-4 py-2.5 text-sm text-slate-200 placeholder:text-slate-500 focus:outline-none focus:border-glow-cyan/30 focus:shadow-glow-sm"
          />
        </div>
        <div className="flex items-center gap-1 flex-wrap">
          <Filter className="h-3.5 w-3.5 text-slate-500 mr-1" />
          {ACTION_CATEGORIES.map((cat) => (
            <button
              key={cat.value}
              onClick={() => {
                setActionFilter(cat.value);
                setVisibleCount(PAGE_SIZE);
              }}
              className={cn(
                "px-2.5 py-1.5 rounded-md text-[11px] font-medium transition-colors",
                actionFilter === cat.value
                  ? "bg-white/[0.08] text-slate-200"
                  : "text-slate-500 hover:text-slate-300"
              )}
            >
              {cat.label}
            </button>
          ))}
        </div>
      </div>

      {/* Error */}
      {error && (
        <div className="rounded-lg border border-red-500/20 bg-red-500/5 p-4 text-sm text-red-400">
          {error}
        </div>
      )}

      {/* Event list */}
      {loading ? (
        <div className="flex flex-col gap-3">
          {[1, 2, 3, 4, 5].map((i) => (
            <Card key={i} className="border-white/[0.08] bg-white/[0.02]">
              <CardContent className="p-4">
                <div className="flex items-start gap-3">
                  <Skeleton className="h-8 w-8 rounded-lg" />
                  <div className="flex-1 space-y-2">
                    <Skeleton className="h-4 w-48" />
                    <Skeleton className="h-3 w-32" />
                  </div>
                </div>
              </CardContent>
            </Card>
          ))}
        </div>
      ) : filteredEntries.length > 0 ? (
        <>
          <p className="text-xs text-slate-500">
            Showing {visibleEntries.length} of {filteredEntries.length} event
            {filteredEntries.length !== 1 ? "s" : ""}
            {actionFilter !== "all" && ` (filtered by ${actionFilter})`}
          </p>
          <div className="flex flex-col gap-2">
            {visibleEntries.map((entry, i) => (
              <AuditEventRow key={entry.id} entry={entry} index={i} />
            ))}
          </div>
          {hasMore && (
            <div className="flex justify-center pt-2">
              <Button
                variant="ghost"
                size="sm"
                className="text-cyan-400 hover:text-cyan-300"
                onClick={() => setVisibleCount((prev) => prev + PAGE_SIZE)}
              >
                <ChevronDown className="h-4 w-4 mr-1.5" />
                Load more ({filteredEntries.length - visibleCount} remaining)
              </Button>
            </div>
          )}
        </>
      ) : (
        <div className="flex flex-col items-center justify-center py-16 text-slate-500">
          <Shield className="h-10 w-10 mb-3 opacity-40" />
          <p className="text-sm font-medium">No audit events found</p>
          <p className="text-xs mt-1">
            {searchQuery || actionFilter !== "all"
              ? "Try adjusting your search or filter criteria."
              : "Audit events will appear as actions are performed in the project."}
          </p>
        </div>
      )}
    </div>
  );
}
