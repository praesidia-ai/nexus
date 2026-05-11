"use client";

import { useState, useEffect, useCallback } from "react";
import { useParams } from "next/navigation";
import {
  Brain,
  TrendingUp,
  TrendingDown,
  Minus,
  Zap,
  BookOpen,
  CheckCircle2,
  XCircle,
  Award,
  Loader2,
  RefreshCw,
  ArrowUpRight,
  Sparkles,
} from "lucide-react";
import { cn } from "@/lib/utils";
import {
  api,
  type SelfImprovementReport,
  type LearnedSkill,
} from "@/lib/api";
import { Card, CardHeader, CardTitle, CardContent } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { useToast } from "@/components/toast";

export default function LearningPage() {
  const { projectId: _projectId } = useParams<{ projectId: string }>();
  const { toast } = useToast();
  const [loading, setLoading] = useState(true);
  const [report, setReport] = useState<SelfImprovementReport | null>(null);
  const [skills, setSkills] = useState<LearnedSkill[]>([]);
  const [promoting, setPromoting] = useState(false);
  const [promotionResult, setPromotionResult] = useState<string | null>(null);

  const fetchData = useCallback(async () => {
    try {
      const [rep, sk] = await Promise.allSettled([
        api.selfImprovementReport(),
        api.selfImprovementSkills(),
      ]);
      if (rep.status === "fulfilled") setReport(rep.value);
      if (sk.status === "fulfilled") setSkills(sk.value.skills ?? []);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    fetchData();
  }, [fetchData]);

  async function handlePromote() {
    setPromoting(true);
    setPromotionResult(null);
    try {
      const result = await api.selfImprovementPromote();
      setPromotionResult(result.message);
      await fetchData();
    } catch (err) {
      toast("error", "Promotion failed", err instanceof Error ? err.message : "Request failed");
    } finally {
      setPromoting(false);
    }
  }

  async function handleRecordUsage(skillId: string, success: boolean) {
    try {
      await api.selfImprovementRecordUsage(skillId, success);
      await fetchData();
    } catch (err) {
      toast("error", "Record failed", err instanceof Error ? err.message : "Request failed");
    }
  }

  function trendIcon(trend: string) {
    if (trend === "improving") return <TrendingUp className="w-4 h-4 text-emerald-400" />;
    if (trend === "declining") return <TrendingDown className="w-4 h-4 text-red-400" />;
    return <Minus className="w-4 h-4 text-slate-400" />;
  }

  function trendColor(trend: string) {
    if (trend === "improving") return "text-emerald-400";
    if (trend === "declining") return "text-red-400";
    return "text-slate-400";
  }

  function skillStatusBadge(status: string) {
    switch (status) {
      case "active":
        return <Badge className="bg-emerald-500/20 text-emerald-300 text-[10px]">Active</Badge>;
      case "pending_eval":
        return <Badge className="bg-amber-500/20 text-amber-300 text-[10px]">Pending Eval</Badge>;
      case "promoted":
        return <Badge className="bg-blue-500/20 text-blue-300 text-[10px]">Promoted</Badge>;
      default:
        return <Badge className="bg-slate-500/20 text-slate-300 text-[10px]">{status}</Badge>;
    }
  }

  if (loading) {
    return (
      <div className="flex-1 flex items-center justify-center">
        <Loader2 className="w-6 h-6 animate-spin text-slate-400" />
      </div>
    );
  }

  const activeSkills = skills.filter(s => s.status === "active" || s.status === "promoted");
  const pendingSkills = skills.filter(s => s.status === "pending_eval");

  return (
    <div className="flex-1 overflow-y-auto">
      <div className="max-w-6xl mx-auto p-6 space-y-6">
        {/* Header */}
        <div className="flex items-center justify-between">
          <div>
            <h1 className="text-2xl font-bold text-white flex items-center gap-3">
              <Brain className="w-7 h-7 text-violet-400" />
              Self-Improving Memory
            </h1>
            <p className="text-sm text-slate-400 mt-1">
              Agents learn from every execution — patterns extracted, skills promoted, performance tracked
            </p>
          </div>
          <div className="flex items-center gap-2">
            <Button variant="outline" size="sm" onClick={fetchData} className="border-white/10 hover:bg-white/5">
              <RefreshCw className="w-4 h-4 mr-1" /> Refresh
            </Button>
            <Button size="sm" onClick={handlePromote} disabled={promoting}>
              {promoting ? <Loader2 className="w-4 h-4 mr-1 animate-spin" /> : <ArrowUpRight className="w-4 h-4 mr-1" />}
              Run Promotion
            </Button>
          </div>
        </div>

        {promotionResult && (
          <div className="bg-blue-500/10 border border-blue-500/20 rounded-lg p-3 text-sm text-blue-300 flex items-center gap-2">
            <Sparkles className="w-4 h-4" />
            {promotionResult}
          </div>
        )}

        {/* Report stats */}
        {report && (
          <div className="grid grid-cols-1 md:grid-cols-4 gap-4">
            <Card className="bg-white/[0.03] border-white/[0.06]">
              <CardContent className="p-4 flex items-center gap-3">
                <div className="w-10 h-10 rounded-lg bg-violet-500/20 flex items-center justify-center">
                  <Zap className="w-5 h-5 text-violet-400" />
                </div>
                <div>
                  <p className="text-xs text-slate-500">Total Generations</p>
                  <p className="text-lg font-semibold text-white">{report.total_generations}</p>
                </div>
              </CardContent>
            </Card>
            <Card className="bg-white/[0.03] border-white/[0.06]">
              <CardContent className="p-4 flex items-center gap-3">
                <div className={cn("w-10 h-10 rounded-lg flex items-center justify-center",
                  report.avg_taste_score >= 0.7 ? "bg-emerald-500/20" : report.avg_taste_score >= 0.5 ? "bg-amber-500/20" : "bg-red-500/20"
                )}>
                  <Award className={cn("w-5 h-5",
                    report.avg_taste_score >= 0.7 ? "text-emerald-400" : report.avg_taste_score >= 0.5 ? "text-amber-400" : "text-red-400"
                  )} />
                </div>
                <div>
                  <p className="text-xs text-slate-500">Avg Quality Score</p>
                  <p className="text-lg font-semibold text-white">{(report.avg_taste_score * 100).toFixed(0)}%</p>
                </div>
              </CardContent>
            </Card>
            <Card className="bg-white/[0.03] border-white/[0.06]">
              <CardContent className="p-4 flex items-center gap-3">
                <div className={cn("w-10 h-10 rounded-lg flex items-center justify-center",
                  report.build_success_rate >= 0.8 ? "bg-emerald-500/20" : "bg-amber-500/20"
                )}>
                  <CheckCircle2 className={cn("w-5 h-5",
                    report.build_success_rate >= 0.8 ? "text-emerald-400" : "text-amber-400"
                  )} />
                </div>
                <div>
                  <p className="text-xs text-slate-500">Build Success Rate</p>
                  <p className="text-lg font-semibold text-white">{(report.build_success_rate * 100).toFixed(0)}%</p>
                </div>
              </CardContent>
            </Card>
            <Card className="bg-white/[0.03] border-white/[0.06]">
              <CardContent className="p-4 flex items-center gap-3">
                <div className="w-10 h-10 rounded-lg bg-blue-500/20 flex items-center justify-center">
                  {trendIcon(report.trend)}
                </div>
                <div>
                  <p className="text-xs text-slate-500">Trend</p>
                  <p className={cn("text-lg font-semibold capitalize", trendColor(report.trend))}>{report.trend}</p>
                </div>
              </CardContent>
            </Card>
          </div>
        )}

        {/* Weak areas */}
        {report && report.weak_areas.length > 0 && (
          <Card className="bg-amber-500/5 border-amber-500/20">
            <CardHeader>
              <CardTitle className="text-base text-amber-300 flex items-center gap-2">
                <TrendingDown className="w-5 h-5" />
                Areas for Improvement
              </CardTitle>
            </CardHeader>
            <CardContent>
              <div className="flex gap-2 flex-wrap">
                {report.weak_areas.map((area, i) => (
                  <Badge key={i} className="bg-amber-500/10 text-amber-300 text-xs">{area}</Badge>
                ))}
              </div>
            </CardContent>
          </Card>
        )}

        {/* Active skills */}
        <Card className="bg-white/[0.03] border-white/[0.06]">
          <CardHeader>
            <CardTitle className="text-base text-white flex items-center gap-2">
              <BookOpen className="w-5 h-5 text-emerald-400" />
              Active Skills ({activeSkills.length})
            </CardTitle>
          </CardHeader>
          <CardContent>
            {activeSkills.length === 0 ? (
              <div className="text-center py-8 text-slate-500">
                <Brain className="w-8 h-8 mx-auto mb-2 opacity-30" />
                <p className="text-sm">No active skills yet</p>
                <p className="text-xs mt-1">Skills are learned from successful executions and promoted after evaluation</p>
              </div>
            ) : (
              <div className="space-y-2">
                {activeSkills.map(skill => (
                  <div key={skill.id} className="flex items-center justify-between p-3 rounded-lg bg-white/[0.02] border border-white/[0.04]">
                    <div className="flex-1">
                      <div className="flex items-center gap-2">
                        <p className="text-sm font-medium text-white">{skill.name}</p>
                        {skillStatusBadge(skill.status)}
                      </div>
                      <div className="flex gap-1 mt-1.5 flex-wrap">
                        {skill.tags?.map(t => (
                          <Badge key={t} variant="secondary" className="text-[10px]">{t}</Badge>
                        ))}
                      </div>
                      <div className="flex items-center gap-4 mt-1.5 text-xs text-slate-500">
                        <span className="flex items-center gap-1">
                          <CheckCircle2 className="w-3 h-3 text-emerald-400" /> {skill.success_count} success
                        </span>
                        <span className="flex items-center gap-1">
                          <XCircle className="w-3 h-3 text-red-400" /> {skill.failure_count} failure
                        </span>
                        <span>{skill.patterns?.length ?? 0} patterns</span>
                      </div>
                    </div>
                    <div className="flex items-center gap-1">
                      <button
                        onClick={() => handleRecordUsage(skill.id, true)}
                        className="p-1.5 rounded-lg text-slate-400 hover:text-emerald-400 hover:bg-emerald-500/10 transition-colors"
                        title="Record success"
                      >
                        <CheckCircle2 className="w-4 h-4" />
                      </button>
                      <button
                        onClick={() => handleRecordUsage(skill.id, false)}
                        className="p-1.5 rounded-lg text-slate-400 hover:text-red-400 hover:bg-red-500/10 transition-colors"
                        title="Record failure"
                      >
                        <XCircle className="w-4 h-4" />
                      </button>
                    </div>
                  </div>
                ))}
              </div>
            )}
          </CardContent>
        </Card>

        {/* Pending skills */}
        {pendingSkills.length > 0 && (
          <Card className="bg-white/[0.03] border-white/[0.06]">
            <CardHeader>
              <CardTitle className="text-base text-white flex items-center gap-2">
                <Zap className="w-5 h-5 text-amber-400" />
                Pending Evaluation ({pendingSkills.length})
              </CardTitle>
            </CardHeader>
            <CardContent>
              <div className="space-y-2">
                {pendingSkills.map(skill => (
                  <div key={skill.id} className="flex items-center justify-between p-3 rounded-lg bg-white/[0.02] border border-white/[0.04]">
                    <div>
                      <div className="flex items-center gap-2">
                        <p className="text-sm font-medium text-white">{skill.name}</p>
                        {skillStatusBadge(skill.status)}
                      </div>
                      <p className="text-xs text-slate-500 mt-1">{skill.patterns?.length ?? 0} patterns discovered</p>
                    </div>
                  </div>
                ))}
              </div>
              <p className="text-xs text-slate-500 mt-3">
                Click &quot;Run Promotion&quot; to evaluate pending skills and promote those meeting quality thresholds.
              </p>
            </CardContent>
          </Card>
        )}
      </div>
    </div>
  );
}
