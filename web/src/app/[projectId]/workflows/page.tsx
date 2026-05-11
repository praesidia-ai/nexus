"use client";

import { useState, useEffect, useCallback } from "react";
import { useParams } from "next/navigation";
import {
  Workflow,
  Play,
  XCircle,
  CheckCircle2,
  Clock,
  Loader2,
  RefreshCw,
  ChevronRight,
  ChevronDown,
  Plus,
  Network,
  Sparkles,
} from "lucide-react";
import { useRouter } from "next/navigation";
import { Card, CardHeader, CardTitle, CardContent, CardDescription } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { useToast } from "@/components/toast";
import { BASE } from "@/lib/api";

// ---------------------------------------------------------------------------
// Types — aligned with backend WorkflowRun + StepMetric
// ---------------------------------------------------------------------------

interface WorkflowRun {
  id: string;
  project_id: string | null;
  pipeline: string;
  status: string; // running | completed | failed
  input_json: string | null;
  output_json: string | null;
  error: string | null;
  started_at: string;
  finished_at: string | null;
}

interface StepMetric {
  id: string;
  run_id: string;
  step_id: string;
  agent: string;
  duration_ms: number;
  retries_used: number;
  skipped: boolean;
  output_length: number;
  status: string; // success | failure | skipped
  error: string | null;
  created_at: string;
}

const PIPELINES = [
  { value: "app-build", label: "App Build", description: "Research → spec → architecture → implement → review" },
  { value: "code-review", label: "Code Review", description: "Multi-pass automated code review" },
  { value: "security-audit", label: "Security Audit", description: "OWASP + dependency + auth review" },
  { value: "feature", label: "Feature", description: "Design + implement a single feature" },
  { value: "deploy", label: "Deploy", description: "Build + test + deploy pipeline" },
];

// ---------------------------------------------------------------------------
// Status helpers
// ---------------------------------------------------------------------------

function statusVariant(status: string) {
  switch (status) {
    case "running":
      return "success" as const;
    case "completed":
    case "success":
      return "secondary" as const;
    case "failed":
    case "failure":
      return "destructive" as const;
    case "skipped":
      return "secondary" as const;
    default:
      return "secondary" as const;
  }
}

function StatusIcon({ status }: { status: string }) {
  switch (status) {
    case "running":
      return <Loader2 className="w-3.5 h-3.5 animate-spin text-emerald-400" />;
    case "completed":
    case "success":
      return <CheckCircle2 className="w-3.5 h-3.5 text-emerald-400" />;
    case "failed":
    case "failure":
      return <XCircle className="w-3.5 h-3.5 text-red-400" />;
    case "skipped":
      return <Clock className="w-3.5 h-3.5 text-slate-500" />;
    default:
      return <Clock className="w-3.5 h-3.5 text-slate-400" />;
  }
}

function formatDuration(ms: number): string {
  if (ms < 1000) return `${ms}ms`;
  if (ms < 60_000) return `${(ms / 1000).toFixed(1)}s`;
  return `${(ms / 60_000).toFixed(1)}m`;
}

// ---------------------------------------------------------------------------
// RunCard
// ---------------------------------------------------------------------------

function RunCard({
  run,
  projectId,
  onCancel,
  actionLoading,
}: {
  run: WorkflowRun;
  projectId: string;
  onCancel: (id: string) => void;
  actionLoading: string | null;
}) {
  const [expanded, setExpanded] = useState(false);
  const [steps, setSteps] = useState<StepMetric[] | null>(null);
  const [stepsLoading, setStepsLoading] = useState(false);
  const isLoading = actionLoading === run.id;
  const isRunning = run.status === "running";

  const loadSteps = useCallback(async () => {
    if (steps !== null) return;
    setStepsLoading(true);
    try {
      const res = await fetch(`${BASE}/projects/${projectId}/workflows/${run.id}/steps`);
      if (res.ok) {
        const data = (await res.json()) as StepMetric[];
        setSteps(Array.isArray(data) ? data : []);
      } else {
        setSteps([]);
      }
    } catch {
      setSteps([]);
    } finally {
      setStepsLoading(false);
    }
  }, [projectId, run.id, steps]);

  useEffect(() => {
    if (expanded) loadSteps();
  }, [expanded, loadSteps]);

  return (
    <Card className="border-white/[0.08] bg-white/[0.02] hover:bg-white/[0.04] transition-colors">
      <CardContent className="p-4">
        <div className="flex items-start justify-between gap-4">
          <div className="flex items-start gap-3 min-w-0 flex-1">
            <button
              onClick={() => setExpanded((e) => !e)}
              className="mt-0.5 rounded-lg bg-cyan-500/10 p-2 shrink-0 hover:bg-cyan-500/20 transition-colors"
              aria-label={expanded ? "Collapse" : "Expand"}
            >
              {expanded ? (
                <ChevronDown className="h-4 w-4 text-cyan-400" />
              ) : (
                <ChevronRight className="h-4 w-4 text-cyan-400" />
              )}
            </button>
            <div className="min-w-0 flex-1">
              <div className="flex items-center gap-2 mb-1">
                <span className="font-medium text-sm truncate">{run.pipeline}</span>
                <Badge variant={statusVariant(run.status)} className="text-[10px]">
                  <StatusIcon status={run.status} />
                  <span className="ml-1">{run.status}</span>
                </Badge>
              </div>
              <div className="flex items-center gap-3 text-xs text-slate-500">
                <span className="flex items-center gap-1">
                  <Clock className="h-3 w-3" />
                  {new Date(run.started_at).toLocaleString()}
                </span>
                {run.finished_at && (
                  <span>
                    {formatDuration(
                      new Date(run.finished_at).getTime() - new Date(run.started_at).getTime()
                    )}
                  </span>
                )}
                {steps && <span>{steps.length} steps</span>}
              </div>
              {run.error && (
                <p className="text-xs text-red-400 mt-1 line-clamp-2">{run.error}</p>
              )}
            </div>
          </div>

          <div className="flex items-center gap-1.5 shrink-0">
            {isRunning && (
              <Button
                variant="ghost"
                size="sm"
                className="h-8 w-8 p-0 text-red-400 hover:text-red-300 hover:bg-red-500/10"
                onClick={() => onCancel(run.id)}
                disabled={isLoading}
                aria-label="Cancel run"
              >
                {isLoading ? (
                  <Loader2 className="h-3.5 w-3.5 animate-spin" />
                ) : (
                  <XCircle className="h-3.5 w-3.5" />
                )}
              </Button>
            )}
          </div>
        </div>

        {expanded && (
          <div className="mt-4 ml-11 border-l border-white/[0.06] pl-4 space-y-3">
            {stepsLoading && (
              <div className="flex items-center gap-2 text-xs text-slate-500">
                <Loader2 className="h-3.5 w-3.5 animate-spin" />
                Loading steps…
              </div>
            )}
            {!stepsLoading && steps && steps.length === 0 && (
              <p className="text-xs text-slate-500">No step metrics recorded yet.</p>
            )}
            {!stepsLoading &&
              steps?.map((step) => (
                <div key={step.id} className="flex items-start gap-3">
                  <StatusIcon status={step.status} />
                  <div className="min-w-0 flex-1">
                    <div className="flex items-center gap-2">
                      <span className="text-xs font-medium text-slate-300">{step.step_id}</span>
                      <span className="text-[10px] text-slate-500">{step.agent}</span>
                      {step.retries_used > 0 && (
                        <Badge variant="warning" className="text-[9px]">
                          {step.retries_used} retries
                        </Badge>
                      )}
                    </div>
                    <div className="flex items-center gap-3 text-[10px] text-slate-500 mt-0.5">
                      <span>{formatDuration(step.duration_ms)}</span>
                      {step.output_length > 0 && <span>{step.output_length} chars</span>}
                    </div>
                    {step.error && (
                      <p className="text-[10px] text-red-400 mt-0.5 line-clamp-2">{step.error}</p>
                    )}
                  </div>
                </div>
              ))}
          </div>
        )}
      </CardContent>
    </Card>
  );
}

// ---------------------------------------------------------------------------
// Start Workflow Dialog
// ---------------------------------------------------------------------------

function StartWorkflowDialog({
  onStart,
  starting,
}: {
  onStart: (pipeline: string) => void;
  starting: boolean;
}) {
  const [open, setOpen] = useState(false);

  if (!open) {
    return (
      <Button
        size="sm"
        className="bg-glow-cyan/10 text-glow-cyan hover:bg-glow-cyan/20"
        onClick={() => setOpen(true)}
        disabled={starting}
      >
        <Plus className="h-4 w-4 mr-1.5" />
        Start Workflow
      </Button>
    );
  }

  return (
    <Card className="border-white/[0.08] bg-white/[0.02]">
      <CardHeader className="pb-2">
        <div className="flex items-center justify-between">
          <CardTitle className="text-sm font-medium">Start New Workflow</CardTitle>
          <Button
            variant="ghost"
            size="sm"
            className="h-6 w-6 p-0 text-slate-500"
            onClick={() => setOpen(false)}
          >
            <XCircle className="h-3.5 w-3.5" />
          </Button>
        </div>
        <CardDescription className="text-xs">Select a pipeline to start</CardDescription>
      </CardHeader>
      <CardContent>
        <div className="grid grid-cols-1 sm:grid-cols-2 gap-2">
          {PIPELINES.map((p) => (
            <button
              key={p.value}
              className="flex flex-col items-start gap-1 p-3 rounded-lg border border-white/[0.08] bg-white/[0.02] hover:bg-white/[0.06] text-left transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
              onClick={() => {
                onStart(p.value);
                setOpen(false);
              }}
              disabled={starting}
            >
              <div className="flex items-center gap-2 text-sm font-medium text-slate-200">
                {starting ? (
                  <Loader2 className="h-3.5 w-3.5 animate-spin" />
                ) : (
                  <Play className="h-3.5 w-3.5 text-glow-cyan" />
                )}
                {p.label}
              </div>
              <p className="text-xs text-slate-500">{p.description}</p>
            </button>
          ))}
        </div>
      </CardContent>
    </Card>
  );
}

// ---------------------------------------------------------------------------
// Page
// ---------------------------------------------------------------------------

export default function WorkflowsPage() {
  const params = useParams();
  const router = useRouter();
  const projectId = params.projectId as string;
  const { toast } = useToast();

  const [runs, setRuns] = useState<WorkflowRun[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [actionLoading, setActionLoading] = useState<string | null>(null);
  const [starting, setStarting] = useState(false);

  const fetchRuns = useCallback(async () => {
    try {
      const res = await fetch(`${BASE}/projects/${projectId}/workflows`);
      if (!res.ok) throw new Error(await res.text());
      const data = await res.json();
      setRuns(Array.isArray(data) ? data : []);
      setError(null);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to load workflows");
    } finally {
      setLoading(false);
    }
  }, [projectId]);

  useEffect(() => {
    fetchRuns();
    const interval = setInterval(fetchRuns, 5000);
    return () => clearInterval(interval);
  }, [fetchRuns]);

  const handleStart = async (pipeline: string) => {
    setStarting(true);
    try {
      const res = await fetch(`${BASE}/projects/${projectId}/workflows`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ pipeline, context: {} }),
      });
      if (!res.ok) throw new Error(await res.text());
      toast("success", "Workflow started", `Pipeline "${pipeline}" is running`);
      await fetchRuns();
    } catch (err) {
      const msg = err instanceof Error ? err.message : "Failed to start workflow";
      toast("error", "Could not start workflow", msg);
    } finally {
      setStarting(false);
    }
  };

  const handleCancel = async (id: string) => {
    setActionLoading(id);
    try {
      const res = await fetch(`${BASE}/projects/${projectId}/workflows/${id}/cancel`, {
        method: "POST",
      });
      if (!res.ok) throw new Error(await res.text());
      toast("success", "Workflow cancelled");
      await fetchRuns();
    } catch (err) {
      const msg = err instanceof Error ? err.message : "Failed to cancel";
      toast("error", "Cancel failed", msg);
    } finally {
      setActionLoading(null);
    }
  };

  const runningCount = runs.filter((w) => w.status === "running").length;
  const failedCount = runs.filter((w) => w.status === "failed").length;

  return (
    <div className="flex flex-col gap-6 p-6 overflow-auto h-full">
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-3">
          <div className="w-9 h-9 rounded-lg bg-gradient-to-br from-violet-500/20 to-cyan-500/20 border border-violet-500/10 flex items-center justify-center">
            <Workflow className="w-4 h-4 text-violet-400" />
          </div>
          <div>
            <h1 className="text-lg font-semibold text-slate-200">Workflows</h1>
            <p className="text-xs text-slate-400">
              {loading
                ? "Loading…"
                : `${runs.length} run${runs.length !== 1 ? "s" : ""} — ${runningCount} running, ${failedCount} failed`}
            </p>
          </div>
        </div>
        <div className="flex items-center gap-2">
          <Button
            variant="outline"
            size="sm"
            onClick={() => router.push(`/${projectId}/agents/create`)}
            className="gap-1.5"
          >
            <Sparkles className="h-3.5 w-3.5 text-glow-cyan" />
            Build AI Company
          </Button>
          <Button
            size="sm"
            onClick={() => router.push(`/${projectId}/workflows/compose`)}
            className="gap-1.5 bg-gradient-to-r from-glow-cyan to-glow-blue text-white shadow-glow-sm hover:brightness-110"
          >
            <Network className="h-3.5 w-3.5" />
            Compose Workflow
          </Button>
          <Button
            variant="ghost"
            size="sm"
            className="text-slate-400 hover:text-slate-200"
            onClick={() => {
              setLoading(true);
              fetchRuns();
            }}
          >
            <RefreshCw className={`h-4 w-4 mr-1.5 ${loading ? "animate-spin" : ""}`} />
            Refresh
          </Button>
        </div>
      </div>

      <StartWorkflowDialog onStart={handleStart} starting={starting} />

      {error && (
        <div className="rounded-lg border border-red-500/20 bg-red-500/5 p-4 text-sm text-red-400">
          {error}
        </div>
      )}

      <div className="flex flex-col gap-2">
        {loading ? (
          <>
            {[1, 2, 3].map((i) => (
              <Card key={i} className="border-white/[0.08] bg-white/[0.02]">
                <CardContent className="p-4 space-y-2">
                  <Skeleton className="h-4 w-48" />
                  <Skeleton className="h-3 w-32" />
                </CardContent>
              </Card>
            ))}
          </>
        ) : runs.length === 0 ? (
          <div className="flex flex-col items-center justify-center py-16 text-slate-500">
            <Workflow className="h-10 w-10 mb-3 opacity-40" />
            <p className="text-sm font-medium">No workflow runs yet</p>
            <p className="text-xs mt-1 mb-5">
              Start a pipeline, design an agent team, or compose a custom workflow.
            </p>
            <div className="flex items-center gap-2">
              <Button
                size="sm"
                onClick={() => router.push(`/${projectId}/agents/create`)}
                className="gap-1.5 bg-gradient-to-r from-glow-cyan to-glow-blue text-white"
              >
                <Sparkles className="h-3.5 w-3.5" />
                Build AI Company
              </Button>
              <Button
                variant="outline"
                size="sm"
                onClick={() => router.push(`/${projectId}/workflows/compose`)}
                className="gap-1.5"
              >
                <Network className="h-3.5 w-3.5" />
                Compose Workflow
              </Button>
            </div>
          </div>
        ) : (
          runs.map((run) => (
            <RunCard
              key={run.id}
              run={run}
              projectId={projectId}
              onCancel={handleCancel}
              actionLoading={actionLoading}
            />
          ))
        )}
      </div>
    </div>
  );
}
