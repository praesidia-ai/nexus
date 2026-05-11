"use client";

import { useState, useEffect } from "react";
import {
  ChevronDown,
  ChevronRight,
  Database,
  Bot,
  Layout,
  Plug,
  Check,
  X,
  Folder,
  Layers,
} from "lucide-react";
import { cn } from "@/lib/utils";
import { api, type AgentDefinition, type MaterializedTable } from "@/lib/api";
import { Badge } from "@/components/ui/badge";
import type { PlanData } from "@/lib/unified-types";

interface Props {
  projectId: string;
  projectName: string;
  currentPlan?: PlanData;
  onPlanSectionClick?: (section: string) => void;
}

export function ProjectContextPanel({
  projectId,
  projectName,
  currentPlan,
  onPlanSectionClick: _onPlanSectionClick,
}: Props) {
  const [agents, setAgents] = useState<AgentDefinition[]>([]);
  const [tables, setTables] = useState<MaterializedTable[]>([]);
  const [planOpen, setPlanOpen] = useState(true);
  const [entitiesOpen, setEntitiesOpen] = useState(false);
  const [agentsOpen, setAgentsOpen] = useState(false);
  const [pagesOpen, setPagesOpen] = useState(false);

  useEffect(() => {
    api.listAgents(projectId).then(setAgents).catch(() => {});
    api.listTables(projectId).then(setTables).catch(() => {});
  }, [projectId]);

  return (
    <div className="w-[260px] flex-shrink-0 border-r border-white/[0.06] flex flex-col h-full overflow-hidden bg-white/[0.01]">
      {/* Project header */}
      <div className="px-4 py-4 border-b border-white/[0.06]">
        <div className="flex items-center gap-2.5">
          <div className="w-8 h-8 rounded-lg bg-glow-cyan/[0.08] border border-glow-cyan/20 flex items-center justify-center flex-shrink-0">
            <Folder className="w-4 h-4 text-glow-cyan" />
          </div>
          <div className="min-w-0">
            <h2 className="text-sm font-semibold text-slate-200 truncate">
              {projectName}
            </h2>
            <p className="text-[11px] text-slate-400">Project context</p>
          </div>
        </div>
      </div>

      {/* Scrollable content */}
      <div className="flex-1 overflow-y-auto scrollbar-thin py-2">
        {/* Plan overview */}
        {currentPlan && (
          <div className="px-3 mb-2">
            <button
              onClick={() => setPlanOpen(!planOpen)}
              className="w-full flex items-center gap-2 px-2 py-2 rounded-lg hover:bg-white/[0.04] transition-colors"
            >
              {planOpen ? (
                <ChevronDown className="w-3.5 h-3.5 text-slate-400" />
              ) : (
                <ChevronRight className="w-3.5 h-3.5 text-slate-400" />
              )}
              <Layers className="w-3.5 h-3.5 text-glow-cyan" />
              <span className="text-xs font-semibold text-slate-200">Plan</span>
            </button>

            {planOpen && (
              <div className="ml-4 mt-1 space-y-1">
                {currentPlan.summary && (
                  <p className="text-[11px] text-slate-400 leading-relaxed px-2 py-1.5 rounded-md bg-white/[0.02]">
                    {currentPlan.summary}
                  </p>
                )}

                {/* Architecture badges */}
                {currentPlan.architecture && (
                  <div className="flex flex-wrap gap-1 px-2 py-1">
                    {currentPlan.architecture.framework && (
                      <Badge variant="outline" className="text-[9px] px-1.5 py-0 gap-1">
                        <Layout className="w-2.5 h-2.5" />
                        {currentPlan.architecture.framework}
                      </Badge>
                    )}
                    {currentPlan.architecture.database && (
                      <Badge variant="outline" className="text-[9px] px-1.5 py-0 gap-1">
                        <Database className="w-2.5 h-2.5" />
                        {currentPlan.architecture.database}
                      </Badge>
                    )}
                    {currentPlan.architecture.auth !== undefined && (
                      <Badge
                        variant={currentPlan.architecture.auth ? "success" : "secondary"}
                        className="text-[9px] px-1.5 py-0 gap-1"
                      >
                        {currentPlan.architecture.auth ? <Check className="w-2.5 h-2.5" /> : <X className="w-2.5 h-2.5" />}
                        Auth
                      </Badge>
                    )}
                  </div>
                )}

                {/* Entities */}
                {currentPlan.entities && currentPlan.entities.length > 0 && (
                  <div>
                    <button
                      onClick={() => setEntitiesOpen(!entitiesOpen)}
                      className="flex items-center gap-1.5 px-2 py-1 text-[11px] text-slate-400 hover:text-slate-200 transition-colors w-full"
                    >
                      {entitiesOpen ? <ChevronDown className="w-3 h-3" /> : <ChevronRight className="w-3 h-3" />}
                      <Database className="w-3 h-3" />
                      <span>Entities</span>
                      <Badge variant="secondary" className="text-[9px] px-1 py-0 ml-auto">
                        {currentPlan.entities.length}
                      </Badge>
                    </button>
                    {entitiesOpen && (
                      <div className="ml-5 space-y-0.5">
                        {currentPlan.entities.map((e) => (
                          <div
                            key={e.name}
                            className="text-[10px] text-slate-400 px-2 py-0.5 rounded hover:bg-white/[0.03] cursor-default"
                          >
                            <span className="text-slate-200 font-medium">{e.name}</span>
                            <span className="ml-1 text-slate-400/60">
                              ({e.fields.length} fields)
                            </span>
                          </div>
                        ))}
                      </div>
                    )}
                  </div>
                )}

                {/* Plan Agents */}
                {currentPlan.agents && currentPlan.agents.length > 0 && (
                  <div>
                    <button
                      onClick={() => setAgentsOpen(!agentsOpen)}
                      className="flex items-center gap-1.5 px-2 py-1 text-[11px] text-slate-400 hover:text-slate-200 transition-colors w-full"
                    >
                      {agentsOpen ? <ChevronDown className="w-3 h-3" /> : <ChevronRight className="w-3 h-3" />}
                      <Bot className="w-3 h-3" />
                      <span>Agents</span>
                      <Badge variant="secondary" className="text-[9px] px-1 py-0 ml-auto">
                        {currentPlan.agents.length}
                      </Badge>
                    </button>
                    {agentsOpen && (
                      <div className="ml-5 space-y-0.5">
                        {currentPlan.agents.map((a) => (
                          <div
                            key={a.name}
                            className="text-[10px] text-slate-400 px-2 py-0.5 rounded hover:bg-white/[0.03]"
                          >
                            <span className="text-slate-200 font-medium">{a.name}</span>
                            <span className="ml-1 text-slate-400/60">{a.role}</span>
                          </div>
                        ))}
                      </div>
                    )}
                  </div>
                )}

                {/* Plan Pages */}
                {currentPlan.pages && currentPlan.pages.length > 0 && (
                  <div>
                    <button
                      onClick={() => setPagesOpen(!pagesOpen)}
                      className="flex items-center gap-1.5 px-2 py-1 text-[11px] text-slate-400 hover:text-slate-200 transition-colors w-full"
                    >
                      {pagesOpen ? <ChevronDown className="w-3 h-3" /> : <ChevronRight className="w-3 h-3" />}
                      <Layout className="w-3 h-3" />
                      <span>Pages</span>
                      <Badge variant="secondary" className="text-[9px] px-1 py-0 ml-auto">
                        {currentPlan.pages.length}
                      </Badge>
                    </button>
                    {pagesOpen && (
                      <div className="ml-5 space-y-0.5">
                        {currentPlan.pages.map((p) => (
                          <div
                            key={p.route}
                            className="text-[10px] text-slate-400 px-2 py-0.5 rounded hover:bg-white/[0.03]"
                          >
                            <span className="text-slate-200 font-medium">{p.name}</span>
                            <span className="ml-1 font-mono text-slate-400/60">{p.route}</span>
                          </div>
                        ))}
                      </div>
                    )}
                  </div>
                )}

                {/* Integrations */}
                {currentPlan.integrations && currentPlan.integrations.length > 0 && (
                  <div className="flex flex-wrap gap-1 px-2 py-1">
                    {currentPlan.integrations.map((i) => (
                      <Badge key={i} variant="outline" className="text-[9px] px-1.5 py-0 gap-1">
                        <Plug className="w-2.5 h-2.5" />
                        {i}
                      </Badge>
                    ))}
                  </div>
                )}
              </div>
            )}
          </div>
        )}

        {/* Live resources */}
        <div className="px-3 mt-2">
          <div className="px-2 mb-2">
            <span className="text-[10px] uppercase tracking-wider text-slate-400/50 font-semibold">
              Live Resources
            </span>
          </div>

          {/* Tables */}
          {tables.length > 0 && (
            <div className="space-y-0.5 mb-2">
              {tables.map((t) => (
                <div
                  key={t.id}
                  className="flex items-center gap-2 px-2 py-1 text-[11px] text-slate-400 rounded hover:bg-white/[0.03]"
                >
                  <Database className="w-3 h-3 text-blue-400 flex-shrink-0" />
                  <span className="truncate">{t.table_name}</span>
                </div>
              ))}
            </div>
          )}

          {/* Running agents */}
          {agents.length > 0 && (
            <div className="space-y-0.5">
              {agents.map((a) => (
                <div
                  key={a.id}
                  className="flex items-center gap-2 px-2 py-1 text-[11px] text-slate-400 rounded hover:bg-white/[0.03]"
                >
                  <Bot className="w-3 h-3 text-purple-400 flex-shrink-0" />
                  <span className="truncate flex-1">{a.name}</span>
                  <span
                    className={cn(
                      "w-1.5 h-1.5 rounded-full flex-shrink-0",
                      a.status === "running" ? "bg-emerald-400 animate-pulse" : "bg-slate-600"
                    )}
                  />
                </div>
              ))}
            </div>
          )}

          {tables.length === 0 && agents.length === 0 && (
            <p className="text-[11px] text-slate-400/50 px-2">
              No resources yet
            </p>
          )}
        </div>
      </div>
    </div>
  );
}
