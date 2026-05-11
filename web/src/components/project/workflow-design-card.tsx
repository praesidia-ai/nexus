"use client";

import { useState } from "react";
import { ArrowRight, Workflow, Check, Zap, ChevronDown, ChevronUp } from "lucide-react";
import { cn } from "@/lib/utils";
import { Badge } from "@/components/ui/badge";

interface WorkflowAgent {
  id?: string;
  name: string;
  role: string;
  icon?: string;
}

interface WorkflowConnection {
  from_agent: string;
  to_agent: string;
  condition: string;
  data_passed: string;
}

interface WorkflowDesignData {
  workflow_title: string;
  workflow_description: string;
  execution_mode: string;
  complexity: string;
  agents_created: number;
  agents: WorkflowAgent[];
  connections: WorkflowConnection[];
  tags: string[];
}

const MODE_LABELS: Record<string, string> = {
  sequential: "Step by step",
  parallel: "All at once",
  pipeline: "Pipeline",
  event_driven: "Event-triggered",
};

const COMPLEXITY_COLORS: Record<string, string> = {
  simple: "bg-emerald-500/20 text-emerald-400 border-emerald-500/30",
  moderate: "bg-amber-500/20 text-amber-400 border-amber-500/30",
  complex: "bg-rose-500/20 text-rose-400 border-rose-500/30",
};

export function WorkflowDesignCard({ data }: { data: Record<string, unknown> }) {
  const [expanded, setExpanded] = useState(false);
  const wf = data as unknown as WorkflowDesignData;

  if (!wf.agents || wf.agents.length === 0) return null;

  const agentMap = new Map(wf.agents.map((a, i) => [a.id ?? `agent_${i + 1}`, a]));

  return (
    <div className="mt-3 w-full max-w-md rounded-xl border border-white/[0.08] overflow-hidden">
      {/* Header */}
      <div className="bg-gradient-to-r from-glow-purple/30 to-glow-cyan/20 px-4 py-3 flex items-center gap-2">
        <Workflow className="w-4 h-4 text-glow-purple" />
        <span className="text-white font-semibold text-sm flex-1 truncate">
          {wf.workflow_title || "Agent Workflow"}
        </span>
        <span className={cn(
          "text-[10px] px-2 py-0.5 rounded-full border",
          COMPLEXITY_COLORS[wf.complexity] ?? "bg-white/10 text-slate-400 border-white/20"
        )}>
          {wf.complexity}
        </span>
      </div>

      {/* Description */}
      <div className="px-4 py-2.5 border-b border-white/[0.06]">
        <p className="text-[12px] text-slate-400 leading-relaxed">{wf.workflow_description}</p>
      </div>

      {/* Agent grid */}
      <div className="px-4 py-3 space-y-2 border-b border-white/[0.06]">
        <div className="flex items-center justify-between mb-1">
          <span className="text-[11px] text-slate-500 uppercase tracking-wider font-medium">
            {wf.agents_created} agent{wf.agents_created !== 1 ? "s" : ""} created
          </span>
          <span className="text-[10px] text-slate-500">
            {MODE_LABELS[wf.execution_mode] ?? wf.execution_mode}
          </span>
        </div>

        {wf.agents.map((agent, i) => (
          <div
            key={agent.id ?? i}
            className="flex items-center gap-2.5 px-3 py-2 rounded-lg bg-white/[0.03] border border-white/[0.06]"
          >
            <span className="text-base flex-shrink-0">{agent.icon ?? "🤖"}</span>
            <div className="flex-1 min-w-0">
              <div className="text-[12px] text-slate-200 font-medium truncate">{agent.name}</div>
              <div className="text-[11px] text-slate-500 truncate">{agent.role}</div>
            </div>
            <Check className="w-3.5 h-3.5 text-emerald-400 flex-shrink-0" />
          </div>
        ))}
      </div>

      {/* Connections (expandable) */}
      {wf.connections && wf.connections.length > 0 && (
        <div className="border-b border-white/[0.06]">
          <button
            onClick={() => setExpanded((e) => !e)}
            className="w-full flex items-center gap-2 px-4 py-2 text-[11px] text-slate-500 hover:text-slate-300 transition-colors"
          >
            <ArrowRight className="w-3 h-3" />
            <span>{wf.connections.length} connection{wf.connections.length !== 1 ? "s" : ""}</span>
            {expanded ? <ChevronUp className="w-3 h-3 ml-auto" /> : <ChevronDown className="w-3 h-3 ml-auto" />}
          </button>

          {expanded && (
            <div className="px-4 pb-3 space-y-1.5">
              {wf.connections.map((conn, i) => {
                const from = agentMap.get(conn.from_agent);
                const to = agentMap.get(conn.to_agent);
                return (
                  <div
                    key={i}
                    className="flex items-center gap-1.5 text-[11px] text-slate-400 px-2 py-1.5 rounded bg-white/[0.02]"
                  >
                    <span className="text-xs">{from?.icon ?? "🤖"}</span>
                    <span className="font-medium text-slate-300 truncate max-w-[80px]">
                      {from?.name ?? conn.from_agent}
                    </span>
                    <ArrowRight className="w-2.5 h-2.5 text-glow-cyan/50 flex-shrink-0" />
                    <span className="text-xs">{to?.icon ?? "🤖"}</span>
                    <span className="font-medium text-slate-300 truncate max-w-[80px]">
                      {to?.name ?? conn.to_agent}
                    </span>
                    <span className="text-slate-500 truncate ml-auto">{conn.condition}</span>
                  </div>
                );
              })}
            </div>
          )}
        </div>
      )}

      {/* Tags */}
      {wf.tags && wf.tags.length > 0 && (
        <div className="px-4 py-2.5 flex flex-wrap gap-1.5">
          {wf.tags.map((tag) => (
            <Badge key={tag} variant="outline" className="text-[10px] px-2 py-0 h-5 text-slate-400 border-white/10">
              {tag}
            </Badge>
          ))}
        </div>
      )}

      {/* Footer */}
      <div className="px-4 py-2.5 bg-emerald-500/5 flex items-center gap-2">
        <Zap className="w-3.5 h-3.5 text-emerald-400" />
        <span className="text-[12px] text-emerald-400">
          Workflow deployed — all agents are ready
        </span>
      </div>
    </div>
  );
}
