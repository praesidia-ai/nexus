"use client";

import { useState, useEffect, useCallback } from "react";
import { useParams } from "next/navigation";
import {
  Sparkles,
  Play,
  Wand2,
  Loader2,
  AlertTriangle,
  CheckCircle2,
  Clock,
  Code2,
  FileText,
  Palette,
  Search,
  TrendingUp,
} from "lucide-react";
import { cn } from "@/lib/utils";
import { Card, CardHeader, CardTitle, CardContent } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { BASE } from "@/lib/api";
import { useToast } from "@/components/toast";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface TasteReport {
  overall_score: number;
  static_analysis: number;
  content: number;
  code_quality: number;
  visual_design: number;
  issues: TasteIssue[];
}

interface TasteIssue {
  id: string;
  category: string;
  severity: string;
  message: string;
  file?: string;
  line?: number;
}

// Raw backend shapes (see crates/nexus-http/src/taste_engine.rs)
interface RawFinding {
  severity?: string;
  description?: string;
  file?: string;
  line?: number;
}
interface RawStageScore {
  stage?: string;
  total?: number;
  axes?: Record<string, { score?: number; findings?: RawFinding[] }>;
}
interface RawTasteReport {
  overall_score?: number;
  stages?: RawStageScore[];
  findings?: RawFinding[];
}

function normalizeSeverity(raw: string | undefined): string {
  const s = (raw ?? "").toLowerCase();
  if (s === "critical" || s === "major" || s === "error") return "error";
  if (s === "minor" || s === "warning") return "warning";
  return "info";
}

function stageTotal(stages: RawStageScore[] | undefined, name: string): number {
  const s = stages?.find((st) => st.stage === name);
  return Math.round(s?.total ?? 0);
}

function collectFindings(raw: RawTasteReport): TasteIssue[] {
  const seen: TasteIssue[] = [];
  const pushFindings = (arr: RawFinding[] | undefined, category: string) => {
    if (!arr) return;
    arr.forEach((f, i) => {
      seen.push({
        id: `${category}-${i}-${f.file ?? ""}-${f.line ?? ""}`,
        category,
        severity: normalizeSeverity(f.severity),
        message: f.description ?? "",
        file: f.file,
        line: f.line,
      });
    });
  };
  if (raw.findings && raw.findings.length) {
    pushFindings(raw.findings, "general");
  } else if (raw.stages) {
    raw.stages.forEach((st) => {
      if (!st.axes) return;
      Object.values(st.axes).forEach((ax) =>
        pushFindings(ax.findings, st.stage ?? "unknown"),
      );
    });
  }
  return seen;
}

function normalizeReport(raw: RawTasteReport): TasteReport {
  return {
    overall_score: Math.round(raw.overall_score ?? 0),
    static_analysis: stageTotal(raw.stages, "static_analysis"),
    content: stageTotal(raw.stages, "content"),
    code_quality: stageTotal(raw.stages, "code_quality"),
    visual_design: stageTotal(raw.stages, "visual_design"),
    issues: collectFindings(raw),
  };
}

interface ScoreHistoryEntry {
  score: number;
  timestamp: string;
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function scoreColor(score: number) {
  if (score >= 75) return "text-emerald-400";
  if (score >= 50) return "text-amber-400";
  return "text-red-400";
}

function scoreBg(score: number) {
  if (score >= 75) return "from-emerald-500/20 to-emerald-500/5";
  if (score >= 50) return "from-amber-500/20 to-amber-500/5";
  return "from-red-500/20 to-red-500/5";
}

function scoreBorder(score: number) {
  if (score >= 75) return "border-emerald-500/20";
  if (score >= 50) return "border-amber-500/20";
  return "border-red-500/20";
}

function severityVariant(severity: string) {
  switch (severity) {
    case "error":
      return "destructive" as const;
    case "warning":
      return "warning" as const;
    case "info":
      return "secondary" as const;
    default:
      return "secondary" as const;
  }
}

const CATEGORY_ICONS: Record<string, typeof Code2> = {
  static_analysis: Search,
  content: FileText,
  code_quality: Code2,
  visual_design: Palette,
};

// ---------------------------------------------------------------------------
// ScoreCard
// ---------------------------------------------------------------------------

function ScoreCard({ label, score, icon: Icon }: { label: string; score: number; icon: typeof Code2 }) {
  return (
    <Card className={cn("border-white/[0.08] bg-white/[0.02]", scoreBorder(score))}>
      <CardContent className="p-4 text-center">
        <Icon className={cn("w-5 h-5 mx-auto mb-2", scoreColor(score))} />
        <p className={cn("text-2xl font-bold", scoreColor(score))}>{score}</p>
        <p className="text-xs text-slate-400 mt-1">{label}</p>
      </CardContent>
    </Card>
  );
}

// ---------------------------------------------------------------------------
// Page
// ---------------------------------------------------------------------------

export default function QualityPage() {
  const params = useParams();
  const projectId = params.projectId as string;
  const { toast } = useToast();

  const [report, setReport] = useState<TasteReport | null>(null);
  const [history, setHistory] = useState<ScoreHistoryEntry[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [running, setRunning] = useState(false);
  const [redesigning, setRedesigning] = useState(false);

  const fetchReport = useCallback(async () => {
    try {
      const [reportRes, historyRes] = await Promise.allSettled([
        fetch(`${BASE}/projects/${projectId}/taste/report`),
        fetch(`${BASE}/projects/${projectId}/taste/history`),
      ]);

      if (reportRes.status === "fulfilled" && reportRes.value.ok) {
        const envelope = (await reportRes.value.json()) as {
          exists?: boolean;
          overall_score?: number;
          report?: RawTasteReport;
        } & RawTasteReport;
        if (envelope.exists === false) {
          setReport(null);
        } else if (envelope.report) {
          // GET /taste/report envelope: { overall_score, report: TasteReport, ... }
          const merged: RawTasteReport = {
            ...envelope.report,
            overall_score: envelope.report.overall_score ?? envelope.overall_score,
          };
          setReport(normalizeReport(merged));
        } else if (envelope.stages || envelope.overall_score != null) {
          // Raw TasteReport returned directly (POST /taste/score response).
          setReport(normalizeReport(envelope));
        } else {
          setReport(null);
        }
      } else if (reportRes.status === "rejected") {
        throw reportRes.reason instanceof Error
          ? reportRes.reason
          : new Error(String(reportRes.reason));
      }

      if (historyRes.status === "fulfilled" && historyRes.value.ok) {
        const data = (await historyRes.value.json()) as
          | ScoreHistoryEntry[]
          | { history?: ScoreHistoryEntry[]; entries?: ScoreHistoryEntry[] };
        const entries = Array.isArray(data)
          ? data
          : data.history ?? data.entries ?? [];
        setHistory(entries);
      }

      setError(null);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to load quality report");
    } finally {
      setLoading(false);
    }
  }, [projectId]);

  useEffect(() => {
    fetchReport();
  }, [fetchReport]);

  const handleRunCheck = async () => {
    setRunning(true);
    try {
      const res = await fetch(`${BASE}/projects/${projectId}/taste/score`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: "{}",
      });
      if (!res.ok) throw new Error((await res.text()) || `HTTP ${res.status}`);
      toast("success", "Quality check complete");
      await fetchReport();
    } catch (err) {
      const msg = err instanceof Error ? err.message : "Quality check failed";
      setError(msg);
      toast("error", "Quality check failed", msg);
    } finally {
      setRunning(false);
    }
  };

  const handleRedesign = async () => {
    setRedesigning(true);
    try {
      const res = await fetch(`${BASE}/projects/${projectId}/taste/redesign`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: "{}",
      });
      if (!res.ok) throw new Error((await res.text()) || `HTTP ${res.status}`);
      toast("success", "Redesign triggered");
      await fetchReport();
    } catch (err) {
      const msg = err instanceof Error ? err.message : "Redesign failed";
      setError(msg);
      toast("error", "Redesign failed", msg);
    } finally {
      setRedesigning(false);
    }
  };

  if (loading) {
    return (
      <div className="p-6 space-y-6">
        <div className="flex items-center gap-3">
          <Skeleton className="h-8 w-48" />
        </div>
        <Skeleton className="h-32 rounded-xl" />
        <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
          {[1, 2, 3, 4].map((i) => (
            <Skeleton key={i} className="h-28 rounded-xl" />
          ))}
        </div>
        <Skeleton className="h-60 rounded-xl" />
      </div>
    );
  }

  return (
    <div className="flex flex-col gap-6 p-6 overflow-auto h-full">
      {/* Header */}
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-3">
          <div className="w-9 h-9 rounded-lg bg-gradient-to-br from-pink-500/20 to-violet-500/20 border border-pink-500/10 flex items-center justify-center">
            <Sparkles className="w-4 h-4 text-pink-400" />
          </div>
          <div>
            <h1 className="text-lg font-semibold text-slate-200">Quality / Taste</h1>
            <p className="text-xs text-slate-400">Heuristic scoring and auto-redesign</p>
          </div>
        </div>
        <div className="flex items-center gap-2">
          <Button
            size="sm"
            className="bg-glow-cyan/10 text-glow-cyan hover:bg-glow-cyan/20"
            onClick={handleRunCheck}
            disabled={running}
          >
            {running ? (
              <Loader2 className="h-3.5 w-3.5 animate-spin mr-1.5" />
            ) : (
              <Play className="h-3.5 w-3.5 mr-1.5" />
            )}
            Run Quality Check
          </Button>
          {report && report.overall_score < 50 && (
            <Button
              size="sm"
              variant="outline"
              className="text-amber-400 border-amber-500/20 hover:bg-amber-500/10"
              onClick={handleRedesign}
              disabled={redesigning}
            >
              {redesigning ? (
                <Loader2 className="h-3.5 w-3.5 animate-spin mr-1.5" />
              ) : (
                <Wand2 className="h-3.5 w-3.5 mr-1.5" />
              )}
              Auto-Redesign
            </Button>
          )}
        </div>
      </div>

      {/* Error */}
      {error && (
        <div className="rounded-lg border border-red-500/20 bg-red-500/5 p-4 text-sm text-red-400">
          {error}
        </div>
      )}

      {report ? (
        <>
          {/* Overall Score */}
          <Card className={cn("border bg-gradient-to-br", scoreBg(report.overall_score), scoreBorder(report.overall_score))}>
            <CardContent className="py-8 text-center">
              <p className={cn("text-6xl font-bold", scoreColor(report.overall_score))}>
                {report.overall_score}
              </p>
              <p className="text-sm text-slate-400 mt-2">Overall Quality Score</p>
              {report.overall_score >= 75 && (
                <div className="flex items-center justify-center gap-1.5 mt-3">
                  <CheckCircle2 className="w-4 h-4 text-emerald-400" />
                  <span className="text-xs text-emerald-400">Quality looks great</span>
                </div>
              )}
              {report.overall_score < 50 && (
                <div className="flex items-center justify-center gap-1.5 mt-3">
                  <AlertTriangle className="w-4 h-4 text-red-400" />
                  <span className="text-xs text-red-400">Needs improvement — consider auto-redesign</span>
                </div>
              )}
            </CardContent>
          </Card>

          {/* Sub-scores */}
          <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
            <ScoreCard label="Static Analysis" score={report.static_analysis} icon={Search} />
            <ScoreCard label="Content" score={report.content} icon={FileText} />
            <ScoreCard label="Code Quality" score={report.code_quality} icon={Code2} />
            <ScoreCard label="Visual Design" score={report.visual_design} icon={Palette} />
          </div>

          {/* Score History */}
          {history.length > 0 && (
            <Card className="border-white/[0.08] bg-white/[0.02]">
              <CardHeader>
                <CardTitle className="text-sm font-medium text-slate-200 flex items-center gap-2">
                  <TrendingUp className="w-4 h-4 text-slate-400" />
                  Score History
                </CardTitle>
              </CardHeader>
              <CardContent>
                <div className="max-h-40 overflow-y-auto space-y-1.5 pr-1">
                  {history.map((entry, i) => (
                    <div
                      key={i}
                      className="flex items-center justify-between py-1.5 border-b border-white/[0.04] last:border-0"
                    >
                      <span className="flex items-center gap-2">
                        <span className={cn("text-sm font-semibold", scoreColor(entry.score))}>
                          {entry.score}
                        </span>
                        <div
                          className="h-1.5 rounded-full bg-white/5"
                          style={{ width: "80px" }}
                        >
                          <div
                            className={cn(
                              "h-full rounded-full",
                              entry.score >= 75
                                ? "bg-emerald-500"
                                : entry.score >= 50
                                  ? "bg-amber-500"
                                  : "bg-red-500",
                            )}
                            style={{ width: `${Math.min(entry.score, 100)}%` }}
                          />
                        </div>
                      </span>
                      <span className="text-[10px] text-slate-500 flex items-center gap-1">
                        <Clock className="h-2.5 w-2.5" />
                        {new Date(entry.timestamp).toLocaleString()}
                      </span>
                    </div>
                  ))}
                </div>
              </CardContent>
            </Card>
          )}

          {/* Issues */}
          <Card className="border-white/[0.08] bg-white/[0.02]">
            <CardHeader>
              <CardTitle className="text-sm font-medium text-slate-200 flex items-center gap-2">
                <AlertTriangle className="w-4 h-4 text-slate-400" />
                Issues Found
                {report.issues.length > 0 && (
                  <Badge variant="secondary" className="text-[10px]">
                    {report.issues.length}
                  </Badge>
                )}
              </CardTitle>
            </CardHeader>
            <CardContent>
              {report.issues.length > 0 ? (
                <div className="max-h-64 overflow-y-auto space-y-2 pr-1">
                  {report.issues.map((issue) => {
                    const CategoryIcon = CATEGORY_ICONS[issue.category] ?? AlertTriangle;
                    return (
                      <div
                        key={issue.id}
                        className="flex items-start gap-3 py-2 border-b border-white/[0.04] last:border-0"
                      >
                        <CategoryIcon className="w-3.5 h-3.5 mt-0.5 text-slate-500 shrink-0" />
                        <div className="min-w-0 flex-1">
                          <div className="flex items-center gap-2 mb-0.5">
                            <Badge variant={severityVariant(issue.severity)} className="text-[10px]">
                              {issue.severity}
                            </Badge>
                            <span className="text-[10px] text-slate-500">{issue.category}</span>
                          </div>
                          <p className="text-xs text-slate-300">{issue.message}</p>
                          {issue.file && (
                            <p className="text-[10px] text-slate-500 mt-0.5 font-mono">
                              {issue.file}{issue.line ? `:${issue.line}` : ""}
                            </p>
                          )}
                        </div>
                      </div>
                    );
                  })}
                </div>
              ) : (
                <div className="py-6 text-center">
                  <CheckCircle2 className="w-6 h-6 text-emerald-400/40 mx-auto mb-1.5" />
                  <p className="text-xs text-slate-500">No issues found</p>
                </div>
              )}
            </CardContent>
          </Card>
        </>
      ) : (
        <div className="flex flex-col items-center justify-center py-16 text-slate-500">
          <Sparkles className="h-10 w-10 mb-3 opacity-40" />
          <p className="text-sm font-medium">No quality report yet</p>
          <p className="text-xs mt-1">Run a quality check to get started</p>
        </div>
      )}
    </div>
  );
}
