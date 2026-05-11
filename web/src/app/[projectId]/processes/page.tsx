"use client";

import { useState, useEffect, useCallback } from "react";
import {
  Cpu,
  Pause,
  Play,
  Skull,
  RefreshCw,
  Loader2,
  Activity,
  DollarSign,
  Zap,
} from "lucide-react";
import { cn } from "@/lib/utils";
import { api, type KernelProcess } from "@/lib/api";
import { useToast } from "@/components/toast";
import { PageHeader } from "@/components/page-header";
import { Card, CardContent } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";

/* ------------------------------------------------------------------ */
/*  Constants                                                          */
/* ------------------------------------------------------------------ */

const STATE_BADGE: Record<string, "success" | "secondary" | "destructive" | "warning" | "default"> = {
  ready: "secondary",
  running: "success",
  waiting: "warning",
  suspended: "warning",
  sleeping: "secondary",
  completed: "secondary",
  failed: "destructive",
  killed: "destructive",
  terminated: "destructive",
  queued: "secondary",
  idle: "secondary",
};

const STATE_DOT: Record<string, string> = {
  ready: "bg-slate-500",
  running: "bg-emerald-400 animate-pulse",
  waiting: "bg-amber-400 animate-pulse",
  suspended: "bg-amber-400",
  sleeping: "bg-slate-500",
  completed: "bg-slate-600",
  failed: "bg-red-400",
  killed: "bg-red-500",
  terminated: "bg-red-500",
  queued: "bg-slate-500",
  idle: "bg-slate-600",
};

/* ------------------------------------------------------------------ */
/*  ProcessCard                                                        */
/* ------------------------------------------------------------------ */

function ProcessCard({
  process,
  onSuspend,
  onResume,
  onKill,
  actionLoading,
}: {
  process: KernelProcess;
  onSuspend: (pid: string) => void;
  onResume: (pid: string) => void;
  onKill: (pid: string) => void;
  actionLoading: string | null;
}) {
  const isLoading = actionLoading === process.pid;
  const isRunning = process.state === "running";
  const isSuspended = process.state === "suspended";
  const isTerminal = process.state === "completed" || process.state === "failed";

  return (
    <Card className="border-white/[0.08] bg-white/[0.02] hover:bg-white/[0.04] transition-colors">
      <CardContent className="p-4">
        <div className="flex items-start justify-between gap-4">
          {/* Left: process info */}
          <div className="flex items-start gap-3 min-w-0 flex-1">
            <div className="mt-0.5 rounded-lg bg-cyan-500/10 p-2 shrink-0">
              <Cpu className="h-4 w-4 text-cyan-400" />
            </div>
            <div className="min-w-0 flex-1">
              <div className="flex items-center gap-2 mb-1">
                <span className="font-medium text-sm truncate">
                  {process.agent_name}
                </span>
                <Badge variant={STATE_BADGE[process.state] ?? "secondary"}>
                  <span
                    className={cn(
                      "mr-1.5 inline-block h-1.5 w-1.5 rounded-full",
                      STATE_DOT[process.state] ?? "bg-slate-600"
                    )}
                  />
                  {process.state}
                </Badge>
                <span className="text-xs text-slate-500 font-mono">
                  PID {process.pid}
                </span>
              </div>
              <p className="text-xs text-slate-400 truncate mb-2">
                {process.task || "No active task"}
              </p>
              <div className="flex items-center gap-4 text-xs text-slate-500">
                <span className="flex items-center gap-1">
                  <Zap className="h-3 w-3" />
                  Priority {process.priority}
                </span>
                <span className="flex items-center gap-1">
                  <Activity className="h-3 w-3" />
                  {process.tokens_used.toLocaleString()} tokens
                </span>
                <span className="flex items-center gap-1">
                  <DollarSign className="h-3 w-3" />
                  ${process.cost_used.toFixed(4)}
                </span>
              </div>
            </div>
          </div>

          {/* Right: actions */}
          <div className="flex items-center gap-1.5 shrink-0">
            {isRunning && (
              <Button
                variant="ghost"
                size="sm"
                className="h-8 w-8 p-0 text-amber-400 hover:text-amber-300 hover:bg-amber-500/10"
                onClick={() => onSuspend(process.pid)}
                disabled={isLoading}
              >
                {isLoading ? (
                  <Loader2 className="h-3.5 w-3.5 animate-spin" />
                ) : (
                  <Pause className="h-3.5 w-3.5" />
                )}
              </Button>
            )}
            {isSuspended && (
              <Button
                variant="ghost"
                size="sm"
                className="h-8 w-8 p-0 text-emerald-400 hover:text-emerald-300 hover:bg-emerald-500/10"
                onClick={() => onResume(process.pid)}
                disabled={isLoading}
              >
                {isLoading ? (
                  <Loader2 className="h-3.5 w-3.5 animate-spin" />
                ) : (
                  <Play className="h-3.5 w-3.5" />
                )}
              </Button>
            )}
            {!isTerminal && (
              <Button
                variant="ghost"
                size="sm"
                className="h-8 w-8 p-0 text-red-400 hover:text-red-300 hover:bg-red-500/10"
                onClick={() => onKill(process.pid)}
                disabled={isLoading}
              >
                {isLoading ? (
                  <Loader2 className="h-3.5 w-3.5 animate-spin" />
                ) : (
                  <Skull className="h-3.5 w-3.5" />
                )}
              </Button>
            )}
          </div>
        </div>
      </CardContent>
    </Card>
  );
}

/* ------------------------------------------------------------------ */
/*  Loading skeleton                                                   */
/* ------------------------------------------------------------------ */

function ProcessSkeleton() {
  return (
    <Card className="border-white/[0.08] bg-white/[0.02]">
      <CardContent className="p-4">
        <div className="flex items-start gap-3">
          <Skeleton className="h-8 w-8 rounded-lg shrink-0" />
          <div className="flex-1 space-y-2">
            <Skeleton className="h-4 w-32" />
            <Skeleton className="h-3 w-64" />
            <Skeleton className="h-3 w-48" />
          </div>
        </div>
      </CardContent>
    </Card>
  );
}

/* ------------------------------------------------------------------ */
/*  Page                                                               */
/* ------------------------------------------------------------------ */

export default function ProcessesPage() {
  const { toast } = useToast();
  const [processes, setProcesses] = useState<KernelProcess[]>([]);
  const [loading, setLoading] = useState(true);
  const [actionLoading, setActionLoading] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const fetchProcesses = useCallback(async () => {
    try {
      const data = await api.listProcesses();
      setProcesses(Array.isArray(data) ? data : []);
      setError(null);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to load processes");
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    fetchProcesses();
    const interval = setInterval(fetchProcesses, 5000);
    return () => clearInterval(interval);
  }, [fetchProcesses]);

  const runAction = async (
    pid: string,
    label: string,
    fn: (pid: string) => Promise<unknown>,
  ) => {
    setActionLoading(pid);
    try {
      await fn(pid);
      toast("success", `Process ${label}`);
      await fetchProcesses();
    } catch (err) {
      const msg = err instanceof Error ? err.message : `Failed to ${label} process`;
      toast("error", `Could not ${label} process`, msg);
    } finally {
      setActionLoading(null);
    }
  };

  const handleSuspend = (pid: string) => runAction(pid, "suspended", api.suspendProcess);
  const handleResume = (pid: string) => runAction(pid, "resumed", api.resumeProcess);
  const handleKill = (pid: string) => runAction(pid, "killed", api.killProcess);

  const runningCount = processes.filter((p) => p.state === "running").length;
  const suspendedCount = processes.filter((p) => p.state === "suspended").length;
  const totalTokens = processes.reduce((sum, p) => sum + (p.tokens_used ?? 0), 0);
  const totalCost = processes.reduce((sum, p) => sum + (p.cost_used ?? 0), 0);

  return (
    <div className="flex flex-col gap-6 p-6">
      <PageHeader
        title="Kernel Processes"
        subtitle={
          loading
            ? "Loading..."
            : `${processes.length} process${processes.length !== 1 ? "es" : ""} — ${runningCount} running, ${suspendedCount} suspended`
        }
        actions={
          <Button
            variant="ghost"
            size="sm"
            className="text-slate-400 hover:text-slate-200"
            onClick={() => {
              setLoading(true);
              fetchProcesses();
            }}
          >
            <RefreshCw className={cn("h-4 w-4 mr-1.5", loading && "animate-spin")} />
            Refresh
          </Button>
        }
      />

      {/* Summary cards */}
      {!loading && processes.length > 0 && (
        <div className="grid grid-cols-2 gap-3 sm:grid-cols-4">
          <Card className="border-white/[0.08] bg-white/[0.02]">
            <CardContent className="p-3 text-center">
              <p className="text-xs text-slate-500 mb-0.5">Total</p>
              <p className="text-lg font-semibold">{processes.length}</p>
            </CardContent>
          </Card>
          <Card className="border-white/[0.08] bg-white/[0.02]">
            <CardContent className="p-3 text-center">
              <p className="text-xs text-slate-500 mb-0.5">Running</p>
              <p className="text-lg font-semibold text-emerald-400">{runningCount}</p>
            </CardContent>
          </Card>
          <Card className="border-white/[0.08] bg-white/[0.02]">
            <CardContent className="p-3 text-center">
              <p className="text-xs text-slate-500 mb-0.5">Tokens</p>
              <p className="text-lg font-semibold">{totalTokens.toLocaleString()}</p>
            </CardContent>
          </Card>
          <Card className="border-white/[0.08] bg-white/[0.02]">
            <CardContent className="p-3 text-center">
              <p className="text-xs text-slate-500 mb-0.5">Cost</p>
              <p className="text-lg font-semibold">${totalCost.toFixed(4)}</p>
            </CardContent>
          </Card>
        </div>
      )}

      {/* Error */}
      {error && (
        <div className="rounded-lg border border-red-500/20 bg-red-500/5 p-4 text-sm text-red-400">
          {error}
        </div>
      )}

      {/* Process list */}
      <div className="flex flex-col gap-2">
        {loading ? (
          <>
            <ProcessSkeleton />
            <ProcessSkeleton />
            <ProcessSkeleton />
            <ProcessSkeleton />
          </>
        ) : processes.length === 0 ? (
          <div className="flex flex-col items-center justify-center py-16 text-slate-500">
            <Cpu className="h-10 w-10 mb-3 opacity-40" />
            <p className="text-sm font-medium">No active processes</p>
            <p className="text-xs mt-1">Kernel processes will appear here when agents are running.</p>
          </div>
        ) : (
          processes.map((p) => (
            <ProcessCard
              key={p.pid}
              process={p}
              onSuspend={handleSuspend}
              onResume={handleResume}
              onKill={handleKill}
              actionLoading={actionLoading}
            />
          ))
        )}
      </div>
    </div>
  );
}
