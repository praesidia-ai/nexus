"use client";

import { useState, useEffect, useCallback, useRef } from "react";
import {
  Loader2,
  Zap,
  CheckCircle2,
  XCircle,
  Terminal,
  Monitor,
  RefreshCw,
  ExternalLink,
  ChevronDown,
  ChevronRight,
  FileText,
  DollarSign,
  ShieldAlert,
  ShieldCheck,
} from "lucide-react";
import { buildAppPreviewUrl, cn } from "@/lib/utils";
import { api, BASE, type AppInstance } from "@/lib/api";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import type { TimelineItem, AgentRoleType, AgentRunStatus } from "@/lib/unified-types";
import { AgentPipelinePanel } from "./agent-pipeline-panel";

interface Props {
  projectId: string;
  timeline: TimelineItem[];
  agentRunning: boolean;
  streaming: boolean;
  currentIteration?: { num: number; max: number };
  currentPhase?: string;
  currentAgent?: AgentRoleType;
  activeAgents?: Record<string, AgentRunStatus>;
  pipelineMode?: string;
}

export function LiveStatePanel({
  projectId,
  timeline,
  agentRunning,
  streaming,
  currentIteration,
  currentPhase,
  currentAgent,
  activeAgents = {},
  pipelineMode,
}: Props) {
  const [appStatus, setAppStatus] = useState<AppInstance | null>(null);
  const [logsOpen, setLogsOpen] = useState(false);
  const [logs, setLogs] = useState<string>("");
  const logStreamRef = useRef<AbortController | null>(null);

  // Cost intelligence
  const [costSummary, setCostSummary] = useState<{
    total_cost_usd: number;
    by_model: { model: string; cost_usd: number; tokens: number }[];
    today_cost_usd: number;
  } | null>(null);
  const [costBudget, setCostBudget] = useState<{
    daily_limit_usd: number;
    used_today_usd: number;
    remaining_usd: number;
    percentage_used: number;
  } | null>(null);

  // HITL pending gates
  const [pendingGates, setPendingGates] = useState<{
    id: string;
    gate_type: string;
    status: string;
    description?: string;
    created_at: string;
  }[]>([]);

  // Poll app status
  useEffect(() => {
    const check = () => {
      api.appStatus(projectId).then(setAppStatus).catch(() => {});
    };
    check();
    const interval = setInterval(check, 5000);
    return () => clearInterval(interval);
  }, [projectId]);

  // Load costs + gates on mount and periodically
  useEffect(() => {
    const loadExtras = () => {
      api.getCostSummary().then(setCostSummary).catch(() => {});
      api.getCostBudget().then(setCostBudget).catch(() => {});
      api.listPendingGates(projectId).then(setPendingGates).catch(() => {});
    };
    loadExtras();
    const interval = setInterval(loadExtras, 15000);
    return () => clearInterval(interval);
  }, [projectId]);

  // Live log streaming
  const mountedRef = useRef(true);

  const startLogStream = useCallback(() => {
    if (logStreamRef.current) return;
    const controller = new AbortController();
    logStreamRef.current = controller;
    fetch(`${BASE}/projects/${projectId}/app/logs/stream`, {
      signal: controller.signal,
    }).then(async (resp) => {
      if (!resp.ok) return;
      const reader = resp.body!.getReader();
      const decoder = new TextDecoder();
      while (true) {
        if (!mountedRef.current || controller.signal.aborted) break;
        const { done, value } = await reader.read();
        if (done) break;
        if (!mountedRef.current || controller.signal.aborted) break;
        const chunk = decoder.decode(value, { stream: true });
        setLogs((prev) => {
          const updated = prev + chunk;
          return updated.length > 5000 ? updated.slice(-5000) : updated;
        });
      }
    }).catch(() => {}).finally(() => {
      logStreamRef.current = null;
    });
  }, [projectId]);

  const stopLogStream = useCallback(() => {
    logStreamRef.current?.abort();
    logStreamRef.current = null;
  }, []);

  // Cleanup on unmount
  useEffect(() => {
    return () => {
      mountedRef.current = false;
      logStreamRef.current?.abort();
    };
  }, []);

  // Derive stats from timeline
  const toolCalls = timeline.filter((e) => e.type === "agent_tool");
  const successCount = toolCalls.filter((e) => e.toolSuccess === true).length;
  const failCount = toolCalls.filter((e) => e.toolSuccess === false).length;
  const pendingCount = toolCalls.filter((e) => e.toolSuccess === undefined).length;
  const recentTools = [...toolCalls].reverse().slice(0, 10);

  const isActive = agentRunning || streaming;

  return (
    <div className="w-[280px] flex-shrink-0 border-l border-white/[0.06] flex flex-col h-full overflow-hidden bg-white/[0.01]">
      {/* Status header */}
      <div className="px-4 py-3 border-b border-white/[0.06]">
        <div className="flex items-center justify-between mb-2">
          <span className="text-xs font-semibold text-slate-200">Live State</span>
          <div className="flex items-center gap-1.5">
            {isActive ? (
              <div className="flex items-center gap-1">
                <div className="w-2 h-2 rounded-full bg-emerald-400 animate-pulse" />
                <span className="text-[10px] text-emerald-400">Active</span>
              </div>
            ) : (
              <div className="flex items-center gap-1">
                <div className="w-2 h-2 rounded-full bg-slate-600" />
                <span className="text-[10px] text-slate-400">Idle</span>
              </div>
            )}
          </div>
        </div>

        {/* Agent status strip */}
        {(agentRunning || currentIteration || currentPhase) && (
          <div className="space-y-1.5">
            {currentIteration && (
              <div className="flex items-center gap-2 text-[11px]">
                <Loader2 className="w-3 h-3 animate-spin text-violet-400" />
                <span className="text-slate-400">
                  Iteration {currentIteration.num}/{currentIteration.max}
                </span>
                <div className="flex-1 h-1 bg-white/[0.06] rounded-full overflow-hidden">
                  <div
                    className="h-full bg-violet-400 rounded-full transition-all duration-300"
                    style={{ width: `${Math.min(100, (currentIteration.num / currentIteration.max) * 100)}%` }}
                  />
                </div>
              </div>
            )}
            {currentPhase && (
              <div className="flex items-center gap-2 text-[11px]">
                <Loader2 className="w-3 h-3 animate-spin text-violet-400" />
                <span className="text-violet-400 font-medium capitalize">{currentPhase} phase</span>
              </div>
            )}
          </div>
        )}

        {/* Stats bar */}
        {toolCalls.length > 0 && (
          <div className="flex items-center gap-3 mt-2 text-[10px] text-slate-400">
            <div className="flex items-center gap-1">
              <Zap className="w-3 h-3" />
              <span>{toolCalls.length}</span>
            </div>
            {successCount > 0 && (
              <div className="flex items-center gap-1">
                <CheckCircle2 className="w-3 h-3 text-emerald-400" />
                <span>{successCount}</span>
              </div>
            )}
            {failCount > 0 && (
              <div className="flex items-center gap-1">
                <XCircle className="w-3 h-3 text-red-400" />
                <span>{failCount}</span>
              </div>
            )}
            {pendingCount > 0 && (
              <div className="flex items-center gap-1">
                <Loader2 className="w-3 h-3 animate-spin" />
                <span>{pendingCount}</span>
              </div>
            )}
          </div>
        )}
      </div>

      {/* Scrollable content */}
      <div className="flex-1 overflow-y-auto scrollbar-thin">
        {/* Multi-agent pipeline panel */}
        {(Object.keys(activeAgents).length > 0 || agentRunning) && (
          <div className="px-4 py-3 border-b border-white/[0.06]">
            <AgentPipelinePanel
              activeAgents={activeAgents}
              currentAgent={currentAgent}
              currentPhase={currentPhase}
              pipelineMode={pipelineMode}
              isRunning={agentRunning}
            />
          </div>
        )}

        {/* HITL Pending Gates */}
        {pendingGates.length > 0 && (
          <div className="px-3 py-3 border-b border-white/[0.06]">
            <div className="flex items-center gap-1.5 px-1 mb-2">
              <ShieldAlert className="w-3.5 h-3.5 text-amber-400" />
              <span className="text-[10px] uppercase tracking-wider text-amber-400 font-semibold">
                Awaiting Approval
              </span>
              <Badge variant="warning" className="text-[9px] px-1 py-0 ml-auto">
                {pendingGates.length}
              </Badge>
            </div>
            <div className="space-y-1.5">
              {pendingGates.map((gate) => (
                <div
                  key={gate.id}
                  className="rounded-lg border border-amber-500/20 bg-amber-500/[0.03] p-2.5"
                >
                  <div className="flex items-center gap-1.5 mb-1">
                    <ShieldAlert className="w-3 h-3 text-amber-400" />
                    <span className="text-[11px] font-medium text-amber-400 capitalize">
                      {gate.gate_type.replace(/_/g, " ")}
                    </span>
                  </div>
                  {gate.description && (
                    <p className="text-[10px] text-slate-400 mb-2">{gate.description}</p>
                  )}
                  <div className="flex gap-1.5">
                    <Button
                      size="sm"
                      className="h-6 text-[10px] px-2 flex-1"
                      onClick={() => {
                        api.resolveGate(projectId, gate.id, true).then(() => {
                          setPendingGates((prev) => prev.filter((g) => g.id !== gate.id));
                        }).catch(() => {});
                      }}
                    >
                      <ShieldCheck className="w-3 h-3 mr-1" />
                      Approve
                    </Button>
                    <Button
                      variant="outline"
                      size="sm"
                      className="h-6 text-[10px] px-2"
                      onClick={() => {
                        api.resolveGate(projectId, gate.id, false).then(() => {
                          setPendingGates((prev) => prev.filter((g) => g.id !== gate.id));
                        }).catch(() => {});
                      }}
                    >
                      <XCircle className="w-3 h-3" />
                    </Button>
                  </div>
                </div>
              ))}
            </div>
          </div>
        )}

        {/* Cost Intelligence */}
        {costSummary && (
          <div className="px-3 py-3 border-b border-white/[0.06]">
            <div className="flex items-center gap-1.5 px-1 mb-2">
              <DollarSign className="w-3.5 h-3.5 text-emerald-400" />
              <span className="text-[10px] uppercase tracking-wider text-slate-400/50 font-semibold">
                Cost
              </span>
            </div>
            <div className="grid grid-cols-2 gap-2">
              <div className="rounded-lg bg-white/[0.03] border border-white/[0.06] p-2">
                <p className="text-[9px] text-slate-400 uppercase">Today</p>
                <p className="text-sm font-semibold text-slate-200">
                  ${costSummary.today_cost_usd.toFixed(3)}
                </p>
              </div>
              <div className="rounded-lg bg-white/[0.03] border border-white/[0.06] p-2">
                <p className="text-[9px] text-slate-400 uppercase">Total</p>
                <p className="text-sm font-semibold text-slate-200">
                  ${costSummary.total_cost_usd.toFixed(2)}
                </p>
              </div>
            </div>
            {/* Budget bar */}
            {costBudget && costBudget.daily_limit_usd > 0 && (
              <div className="mt-2">
                <div className="flex items-center justify-between text-[9px] text-slate-400 mb-1">
                  <span>Budget</span>
                  <span
                    className={cn(
                      costBudget.percentage_used > 80 ? "text-amber-400" :
                      costBudget.percentage_used > 95 ? "text-red-400" : ""
                    )}
                  >
                    {costBudget.percentage_used.toFixed(0)}%
                  </span>
                </div>
                <div className="h-1.5 bg-white/[0.06] rounded-full overflow-hidden">
                  <div
                    className={cn(
                      "h-full rounded-full transition-all",
                      costBudget.percentage_used > 95 ? "bg-red-400" :
                      costBudget.percentage_used > 80 ? "bg-amber-400" :
                      "bg-emerald-400"
                    )}
                    style={{ width: `${Math.min(100, costBudget.percentage_used)}%` }}
                  />
                </div>
              </div>
            )}
            {/* Per-model breakdown */}
            {costSummary.by_model.length > 0 && (
              <div className="mt-2 space-y-0.5">
                {costSummary.by_model.slice(0, 3).map((m) => (
                  <div key={m.model} className="flex items-center justify-between text-[10px]">
                    <span className="text-slate-400 font-mono truncate">{m.model}</span>
                    <span className="text-slate-200">${m.cost_usd.toFixed(3)}</span>
                  </div>
                ))}
              </div>
            )}
          </div>
        )}

        {/* Recent tool calls */}
        {recentTools.length > 0 && (
          <div className="px-3 py-3 border-b border-white/[0.06]">
            <span className="text-[10px] uppercase tracking-wider text-slate-400/50 font-semibold px-1">
              Recent Actions
            </span>
            <div className="mt-2 space-y-1">
              {recentTools.map((tool) => (
                <div
                  key={tool.id}
                  className="flex items-center gap-2 px-2 py-1.5 rounded-lg hover:bg-white/[0.03] transition-colors"
                >
                  <div
                    className={cn(
                      "w-1.5 h-1.5 rounded-full flex-shrink-0",
                      tool.toolSuccess === true
                        ? "bg-emerald-400"
                        : tool.toolSuccess === false
                        ? "bg-red-400"
                        : "bg-violet-400 animate-pulse"
                    )}
                  />
                  <span className="font-mono text-[10px] text-slate-200 truncate flex-1">
                    {tool.toolName}
                  </span>
                  {tool.toolDuration != null && (
                    <span className="text-[9px] text-slate-400 font-mono flex-shrink-0">
                      {tool.toolDuration < 1000
                        ? `${tool.toolDuration}ms`
                        : `${(tool.toolDuration / 1000).toFixed(1)}s`}
                    </span>
                  )}
                </div>
              ))}
            </div>
          </div>
        )}

        {/* App Preview section */}
        <div className="px-3 py-3 border-b border-white/[0.06]">
          <span className="text-[10px] uppercase tracking-wider text-slate-400/50 font-semibold px-1">
            App Preview
          </span>
          <div className="mt-2">
            {appStatus && appStatus.status === "running" ? (
              <div className="space-y-2">
                <div className="rounded-lg border border-emerald-500/20 bg-emerald-500/[0.03] p-3">
                  <div className="flex items-center gap-2 mb-2">
                    <Monitor className="w-3.5 h-3.5 text-emerald-400" />
                    <span className="text-xs font-medium text-emerald-400">Running</span>
                    <span className="text-[10px] text-slate-400 ml-auto font-mono">
                      :{appStatus.port}
                    </span>
                  </div>
                  <div className="flex gap-1.5">
                    <Button
                      variant="outline"
                      size="sm"
                      className="text-[10px] h-6 px-2 flex-1"
                      asChild
                    >
                      <a
                        href={buildAppPreviewUrl(appStatus.port, appStatus.url) ?? "#"}
                        target="_blank"
                        rel="noopener noreferrer"
                      >
                        <ExternalLink className="w-3 h-3 mr-1" />
                        Open
                      </a>
                    </Button>
                    <Button
                      variant="outline"
                      size="sm"
                      className="text-[10px] h-6 px-2"
                      onClick={() => {
                        api.restartApp(projectId).then(setAppStatus).catch(() => {});
                      }}
                    >
                      <RefreshCw className="w-3 h-3" />
                    </Button>
                  </div>
                </div>

                {/* Live logs toggle */}
                <button
                  onClick={() => {
                    if (!logsOpen) {
                      startLogStream();
                    } else {
                      stopLogStream();
                    }
                    setLogsOpen(!logsOpen);
                  }}
                  className="flex items-center gap-1.5 px-2 py-1 text-[11px] text-slate-400 hover:text-slate-200 transition-colors w-full"
                >
                  {logsOpen ? <ChevronDown className="w-3 h-3" /> : <ChevronRight className="w-3 h-3" />}
                  <Terminal className="w-3 h-3" />
                  <span>Live Logs</span>
                  {logsOpen && (
                    <div className="w-1.5 h-1.5 rounded-full bg-emerald-400 animate-pulse ml-auto" />
                  )}
                </button>
                {logsOpen && (
                  <pre className="text-[9px] font-mono text-slate-400 bg-black/30 rounded-md p-2 max-h-48 overflow-y-auto scrollbar-thin whitespace-pre-wrap break-all">
                    {logs || "Waiting for logs..."}
                  </pre>
                )}
              </div>
            ) : (
              <div className="rounded-lg border border-white/[0.06] bg-white/[0.02] p-3 text-center">
                <Monitor className="w-6 h-6 text-slate-400/20 mx-auto mb-2" />
                <p className="text-[11px] text-slate-400">App not running</p>
              </div>
            )}
          </div>
        </div>

        {/* File changes from timeline */}
        {(() => {
          const fileTools = timeline.filter(
            (t) =>
              t.type === "agent_tool" &&
              (t.toolName === "file_write" || t.toolName === "file_edit") &&
              t.toolSuccess === true
          );
          if (fileTools.length === 0) return null;

          return (
            <div className="px-3 py-3">
              <span className="text-[10px] uppercase tracking-wider text-slate-400/50 font-semibold px-1">
                Files Changed
              </span>
              <div className="mt-2 space-y-0.5">
                {fileTools.slice(-15).map((t) => (
                  <div
                    key={t.id}
                    className="flex items-center gap-2 px-2 py-1 text-[10px] text-slate-400"
                  >
                    <FileText className="w-3 h-3 flex-shrink-0 text-blue-400" />
                    <span className="font-mono truncate">
                      {String(t.toolArgs?.path ?? "")}
                    </span>
                  </div>
                ))}
              </div>
            </div>
          );
        })()}
      </div>
    </div>
  );
}
