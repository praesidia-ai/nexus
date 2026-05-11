"use client";

import { useState, useEffect } from "react";
import { BarChart3, Bot, Database, Brain, Loader2 } from "lucide-react";
import { cn } from "@/lib/utils";
import {
  api,
  type AgentDefinition,
  type MaterializedTable,
  type KnowledgeItem,
  type WorkflowRun,
} from "@/lib/api";
import { Card, CardHeader, CardContent, CardTitle } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { EmptyState } from "@/components/empty-state";

interface KpiCard {
  label: string;
  value: number;
  icon: React.ElementType;
  iconBg: string;
  iconColor: string;
}

const STATUS_VARIANT: Record<string, "success" | "warning" | "destructive" | "secondary"> = {
  completed: "success", running: "warning", failed: "destructive",
};

const AGENT_STATUS_VARIANT: Record<string, "success" | "secondary" | "destructive"> = {
  running: "success", defined: "secondary", stopped: "destructive", error: "destructive",
};

function formatDuration(start: string, end?: string): string {
  if (!end) return "In progress";
  const ms = new Date(end).getTime() - new Date(start).getTime();
  if (ms < 1000) return `${ms}ms`;
  if (ms < 60000) return `${(ms / 1000).toFixed(1)}s`;
  return `${Math.floor(ms / 60000)}m ${Math.round((ms % 60000) / 1000)}s`;
}

function formatDate(iso: string): string {
  const d = new Date(iso);
  return d.toLocaleDateString(undefined, { month: "short", day: "numeric", hour: "2-digit", minute: "2-digit" });
}

export function ProjectMetrics({ projectId }: { projectId: string }) {
  const [runs, setRuns] = useState<WorkflowRun[]>([]);
  const [agents, setAgents] = useState<AgentDefinition[]>([]);
  const [tables, setTables] = useState<MaterializedTable[]>([]);
  const [knowledge, setKnowledge] = useState<KnowledgeItem[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    setLoading(true); setError(null);
    Promise.all([
      api.listWorkflowRuns(projectId).catch(() => [] as WorkflowRun[]),
      api.listAgents(projectId).catch(() => [] as AgentDefinition[]),
      api.listTables(projectId).catch(() => [] as MaterializedTable[]),
      api.listKnowledge(projectId).catch(() => [] as KnowledgeItem[]),
    ])
      .then(([r, a, t, k]) => { setRuns(r); setAgents(a); setTables(t); setKnowledge(k); })
      .catch(() => setError("Failed to load metrics data"))
      .finally(() => setLoading(false));
  }, [projectId]);

  const kpis: KpiCard[] = [
    { label: "Workflow Runs", value: runs.length, icon: BarChart3, iconBg: "bg-glow-cyan/[0.08]", iconColor: "text-glow-cyan" },
    { label: "Agents", value: agents.length, icon: Bot, iconBg: "bg-purple-500/10", iconColor: "text-purple-400" },
    { label: "Tables", value: tables.length, icon: Database, iconBg: "bg-emerald-500/10", iconColor: "text-emerald-400" },
    { label: "Knowledge Items", value: knowledge.length, icon: Brain, iconBg: "bg-blue-500/10", iconColor: "text-blue-400" },
  ];

  if (loading) return <div className="h-full flex items-center justify-center"><Loader2 className="w-6 h-6 animate-spin text-slate-400" /></div>;
  if (error) return <div className="p-8"><Card className="p-8 text-center"><p className="text-sm text-destructive">{error}</p></Card></div>;

  return (
    <div className="h-full overflow-y-auto scrollbar-thin p-6">
      <div className="grid grid-cols-4 gap-4 mb-8">
        {kpis.map((kpi) => (
          <Card key={kpi.label} className="glass-card">
            <CardContent className="p-4 pt-4 flex justify-between items-start">
              <div><p className="text-[13px] text-slate-400 mb-1">{kpi.label}</p><p className="text-2xl font-semibold text-slate-200">{kpi.value}</p></div>
              <div className={cn("w-9 h-9 rounded-lg flex items-center justify-center", kpi.iconBg)}><kpi.icon className={cn("w-4.5 h-4.5", kpi.iconColor)} /></div>
            </CardContent>
          </Card>
        ))}
      </div>

      <Card className="glass-card overflow-hidden mb-6">
        <CardHeader className="border-b border-white/[0.06]"><CardTitle className="text-[15px] font-semibold text-slate-200">Workflow Runs</CardTitle></CardHeader>
        {runs.length === 0 ? (
          <div className="px-6 py-8"><EmptyState icon={BarChart3} title="No workflow runs" description="Workflow runs will appear here once pipelines are executed." /></div>
        ) : (
          <table className="w-full">
            <thead><tr className="border-b border-white/[0.06]">
              <th scope="col" className="text-left px-6 py-3 text-[12px] font-medium text-slate-400">Pipeline</th>
              <th scope="col" className="text-left px-6 py-3 text-[12px] font-medium text-slate-400">Status</th>
              <th scope="col" className="text-left px-6 py-3 text-[12px] font-medium text-slate-400">Started</th>
              <th scope="col" className="text-left px-6 py-3 text-[12px] font-medium text-slate-400">Duration</th>
            </tr></thead>
            <tbody>
              {runs.map((run) => (
                <tr key={run.id} className="border-b border-white/[0.06] last:border-0 hover:bg-white/[0.02] transition-colors">
                  <td className="px-6 py-3 text-[13px] font-medium text-slate-200">{run.pipeline}</td>
                  <td className="px-6 py-3"><Badge variant={STATUS_VARIANT[run.status] ?? "secondary"}>{run.status}</Badge></td>
                  <td className="px-6 py-3 text-[13px] text-slate-400">{formatDate(run.started_at)}</td>
                  <td className="px-6 py-3 text-[13px] text-slate-400">{formatDuration(run.started_at, run.finished_at ?? undefined)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </Card>

      <Card className="glass-card overflow-hidden">
        <CardHeader className="border-b border-white/[0.06]"><CardTitle className="text-[15px] font-semibold text-slate-200">Agent Status</CardTitle></CardHeader>
        {agents.length === 0 ? (
          <div className="px-6 py-8"><EmptyState icon={Bot} title="No agents" description="Create agents via the chat to see their status here." /></div>
        ) : (
          <CardContent className="pt-5">
            <div className="grid grid-cols-3 gap-4">
              {agents.map((agent) => (
                <Card key={agent.id} className="glass-card p-4 hover:border-glow-cyan/30 hover:bg-glow-cyan/[0.03] transition-colors">
                  <div className="flex items-center justify-between mb-2">
                    <span className="text-sm font-semibold text-slate-200">{agent.name}</span>
                    <Badge variant={AGENT_STATUS_VARIANT[agent.status] ?? "secondary"}>{agent.status}</Badge>
                  </div>
                  <p className="text-xs text-slate-400 truncate">{agent.role}</p>
                </Card>
              ))}
            </div>
          </CardContent>
        )}
      </Card>
    </div>
  );
}
