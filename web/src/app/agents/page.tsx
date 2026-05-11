"use client";

import { useState } from "react";
import Link from "next/link";
import {
  ArrowLeft,
  Bot,
  Clock,
  Zap,
  ChevronRight,
  X,
  Activity,
} from "lucide-react";
import { useQuery } from "@tanstack/react-query";
import { cn } from "@/lib/utils";
import { api, type AgentDefinition } from "@/lib/api";
import { Badge } from "@/components/ui/badge";
import { Card, CardContent } from "@/components/ui/card";
import { Skeleton } from "@/components/ui/skeleton";
import { EmptyState } from "@/components/empty-state";

interface AgentWithProject extends AgentDefinition {
  projectName: string;
}

const STATUS_BADGE_VARIANT: Record<string, "success" | "secondary" | "destructive" | "warning"> = {
  running: "success",
  defined: "secondary",
  stopped: "destructive",
  error: "destructive",
  idle: "warning",
};

function useAllAgents() {
  return useQuery({
    queryKey: ["all-agents"],
    queryFn: async (): Promise<AgentWithProject[]> => {
      const projects = await api.listProjects();
      const results = await Promise.allSettled(
        projects.map(async (p) => {
          const agents = await api.listAgents(p.id);
          return agents.map((a) => ({ ...a, projectName: p.name }));
        }),
      );
      return results.flatMap((r) => (r.status === "fulfilled" ? r.value : []));
    },
  });
}

export default function AgentsPage() {
  const { data: allAgents = [], isLoading } = useAllAgents();
  const [selectedAgent, setSelectedAgent] = useState<AgentWithProject | null>(null);

  const runningAgents = allAgents.filter((a) => a.status === "running");
  const reactiveAgents = allAgents.filter((a) => a.status === "defined" || a.status === "idle");
  const recentAgents = allAgents.filter((a) => a.status === "stopped" || a.status === "error");

  return (
    <div className="flex h-full overflow-hidden">
      <div className="flex-1 overflow-y-auto scrollbar-thin p-8 max-w-5xl mx-auto">
        <Link
          href="/"
          className="inline-flex items-center gap-1.5 text-sm text-slate-400 hover:text-slate-200 transition-colors mb-6 focus-visible:outline-none focus-visible:text-glow-cyan"
        >
          <ArrowLeft className="w-4 h-4" />
          Back to Home
        </Link>

        <div className="mb-8">
          <h1 className="text-2xl font-bold text-slate-200 mb-1">Agents</h1>
          <p className="text-sm text-slate-400">
            {isLoading ? "Loading agents..." : `${allAgents.length} agents across all projects`}
          </p>
        </div>

        {isLoading ? (
          <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-3">
            {Array.from({ length: 6 }).map((_, i) => (
              <Card key={i}>
                <CardContent className="p-4 flex items-center gap-3">
                  <Skeleton className="w-10 h-10 rounded-lg" />
                  <div className="flex-1 space-y-2">
                    <Skeleton className="h-4 w-24" />
                    <Skeleton className="h-3 w-16" />
                  </div>
                </CardContent>
              </Card>
            ))}
          </div>
        ) : allAgents.length === 0 ? (
          <EmptyState
            icon={Bot}
            title="No agents yet"
            description="Create agents by designing a workflow from a simple idea. Open a project and use the Agent Creator."
          />
        ) : (
          <div className="space-y-8">
            <AgentSection
              icon={<Activity className="w-4 h-4 text-emerald-400" />}
              title="Running Now"
              agents={runningAgents}
              badgeVariant="success"
              onSelect={setSelectedAgent}
            />
            <AgentSection
              icon={<Zap className="w-4 h-4 text-amber-400" />}
              title="Reactive"
              agents={reactiveAgents}
              badgeVariant="warning"
              onSelect={setSelectedAgent}
            />
            <AgentSection
              icon={<Clock className="w-4 h-4 text-slate-400" />}
              title="Recent History"
              agents={recentAgents}
              badgeVariant="secondary"
              onSelect={setSelectedAgent}
            />
          </div>
        )}
      </div>

      {/* Detail slide-out */}
      {selectedAgent && (
        <AgentDetail agent={selectedAgent} onClose={() => setSelectedAgent(null)} />
      )}
    </div>
  );
}

function AgentSection({
  icon,
  title,
  agents,
  badgeVariant,
  onSelect,
}: {
  icon: React.ReactNode;
  title: string;
  agents: AgentWithProject[];
  badgeVariant: "success" | "warning" | "secondary";
  onSelect: (agent: AgentWithProject) => void;
}) {
  if (agents.length === 0) return null;

  return (
    <section>
      <div className="flex items-center gap-2 mb-4">
        {icon}
        <h2 className="text-sm font-semibold text-slate-200 uppercase tracking-wider">{title}</h2>
        <Badge variant={badgeVariant} className="text-[10px]">{agents.length}</Badge>
      </div>
      <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-3">
        {agents.map((agent, i) => (
          <Card
            key={agent.id}
            className="glass-card-hover cursor-pointer group fade-in-up"
            style={{ animationDelay: `${i * 50}ms` }}
            onClick={() => onSelect(agent)}
          >
            <CardContent className="p-4 flex items-center gap-3">
              <div className="w-10 h-10 rounded-lg bg-glow-cyan/[0.08] flex items-center justify-center flex-shrink-0">
                <Bot className={cn("w-5 h-5", agent.status === "running" ? "text-emerald-400" : "text-glow-cyan")} />
              </div>
              <div className="flex-1 min-w-0">
                <div className="flex items-center gap-2">
                  <span className="text-sm font-semibold text-slate-200 truncate">{agent.name}</span>
                  {agent.status === "running" && (
                    <span className="w-1.5 h-1.5 rounded-full bg-emerald-400 animate-pulse flex-shrink-0" />
                  )}
                </div>
                <p className="text-xs text-slate-400 truncate">{agent.role}</p>
                <p className="text-[10px] text-slate-400/50 mt-0.5">{agent.projectName}</p>
              </div>
              <ChevronRight className="w-4 h-4 text-slate-400/40 group-hover:text-glow-cyan transition-colors flex-shrink-0" />
            </CardContent>
          </Card>
        ))}
      </div>
    </section>
  );
}

function AgentDetail({ agent, onClose }: { agent: AgentWithProject; onClose: () => void }) {
  return (
    <div className="w-[380px] flex-shrink-0 glass border-l border-white/[0.06] overflow-y-auto scrollbar-thin animate-slide-in-right">
      <div className="p-6">
        <div className="flex items-center justify-between mb-6">
          <h3 className="text-sm font-semibold text-slate-200">Agent Detail</h3>
          <button
            onClick={onClose}
            className="text-slate-400 hover:text-slate-200 transition-colors"
            aria-label="Close detail panel"
          >
            <X className="w-4 h-4" />
          </button>
        </div>

        <div className="flex items-center gap-3 mb-6">
          <div className="w-12 h-12 rounded-xl bg-glow-cyan/[0.08] flex items-center justify-center">
            <Bot className="w-6 h-6 text-glow-cyan" />
          </div>
          <div>
            <p className="font-semibold text-slate-200">{agent.name}</p>
            <p className="text-xs text-slate-400">{agent.role}</p>
          </div>
        </div>

        <div className="space-y-4">
          <DetailRow label="Status">
            <Badge variant={STATUS_BADGE_VARIANT[agent.status] ?? "secondary"}>{agent.status}</Badge>
          </DetailRow>
          <DetailRow label="Project">{agent.projectName}</DetailRow>
          <DetailRow label="Provider">{agent.provider || "--"}</DetailRow>
          <DetailRow label="Model">{agent.model || "--"}</DetailRow>
          {agent.temperature != null && <DetailRow label="Temperature">{agent.temperature}</DetailRow>}
          {agent.max_tokens != null && <DetailRow label="Max Tokens">{agent.max_tokens}</DetailRow>}
          {agent.zeroclaw_port && <DetailRow label="Port">{agent.zeroclaw_port}</DetailRow>}
          {agent.tools.length > 0 && (
            <div>
              <p className="text-[11px] text-slate-400 uppercase tracking-wider mb-2">Tools</p>
              <div className="flex flex-wrap gap-1.5">
                {agent.tools.map((tool) => (
                  <Badge key={tool} variant="outline" className="text-[10px]">{tool}</Badge>
                ))}
              </div>
            </div>
          )}
          {agent.system_prompt && (
            <div>
              <p className="text-[11px] text-slate-400 uppercase tracking-wider mb-2">System Prompt</p>
              <pre className="text-xs bg-white/[0.02] rounded-lg p-3 text-slate-400 whitespace-pre-wrap max-h-48 overflow-auto scrollbar-thin font-mono">
                {agent.system_prompt.slice(0, 500)}
                {agent.system_prompt.length > 500 ? "..." : ""}
              </pre>
            </div>
          )}
          <DetailRow label="Created">{new Date(agent.created_at).toLocaleDateString()}</DetailRow>
        </div>
      </div>
    </div>
  );
}

function DetailRow({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="flex items-center justify-between">
      <span className="text-[11px] text-slate-400 uppercase tracking-wider">{label}</span>
      <span className="text-sm text-slate-200">{children}</span>
    </div>
  );
}
