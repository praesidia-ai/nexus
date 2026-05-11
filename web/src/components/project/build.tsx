"use client";

import { useState, useEffect, useRef, useCallback } from "react";
import {
  Bot,
  FileCode2,
  FilePlus,
  Sparkles,
  CheckCircle2,
  XCircle,
  Loader2,
  Activity,
  Layers,
  Zap,
  Eye,
  Terminal,
  Bug,
  Brain,
  AlertTriangle,
  Clock,
  ChevronDown,
  ChevronRight,
  Shield,
  DollarSign,
  FileCode,
  RefreshCw,
  GitBranch,
  Monitor,
} from "lucide-react";
import { cn } from "@/lib/utils";
import { api, BASE } from "@/lib/api";
import { AppPreview } from "@/components/app-preview";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Progress } from "@/components/ui/progress";
import { Tabs, TabsList, TabsTrigger, TabsContent } from "@/components/ui/tabs";

// ---------------------------------------------------------------------------
// Live Build types
// ---------------------------------------------------------------------------

type EventType =
  | "agent_action"
  | "file_created"
  | "file_updated"
  | "component_ready"
  | "build_step"
  | "progress"
  | "taste_score"
  | "complete"
  | "error"
  | "lag";

interface AgentActionEvent { type: "agent_action"; agent: string; action: string; target?: string; }
interface FileCreatedEvent { type: "file_created"; path: string; language: string; lines: number; component_type?: string; }
interface FileUpdatedEvent { type: "file_updated"; path: string; change_summary: string; }
interface ComponentReadyEvent { type: "component_ready"; component: string; path: string; preview_hint: string; }
interface BuildStepEvent { type: "build_step"; step: string; status: string; duration_ms?: number; }
interface ProgressEvent { type: "progress"; percent: number; message: string; files_created: number; files_total?: number; }
interface TasteScoreEvent { type: "taste_score"; overall: number; message: string; }
interface CompleteEvent { type: "complete"; files_created: number; duration_ms: number; taste_score?: number; }
interface ErrorEvent { type: "error"; message: string; recoverable: boolean; }

type BuildEvent =
  | AgentActionEvent | FileCreatedEvent | FileUpdatedEvent | ComponentReadyEvent
  | BuildStepEvent | ProgressEvent | TasteScoreEvent | CompleteEvent | ErrorEvent
  | { type: "lag" };

interface LogEntry { id: string; timestamp: Date; event: BuildEvent; }

// ---------------------------------------------------------------------------
// Debugger types
// ---------------------------------------------------------------------------

interface EnforcementResult {
  passed: boolean; score: number; required_score: number;
  blocking_violations: Violation[]; warnings: Violation[]; info: Violation[];
  auto_fixed: number; timestamp: string;
}

interface Violation { rule_id: string; category: string; severity: string; file: string; line?: number; message: string; fix?: string; }
interface CostSummary { date: string; total_calls: number; total_cost_usd: number; total_tokens: number; records: CostRecord[]; }
interface CostRecord { model: string; purpose: string; cost_usd: number; total_tokens: number; timestamp: string; }
interface PipelineEstimate { total_estimated_ms: number; steps: StepEstimate[]; confidence: number; }
interface StepEstimate { name: string; estimated_ms: number; can_parallel: boolean; skeleton_available: boolean; }
interface GateEntry { id: string; gate_type: string; description: string; risk_level: string; status: string; created_at: string; }
interface TraceEntry { id: string; agent_name: string; task_summary: string; status: string; started_at: string; ended_at?: string; total_tokens?: number; }
interface HealingEvent { id: string; trigger: string; status: string; error_log: string; fix_summary?: string; created_at: string; }

// ---------------------------------------------------------------------------
// Live Build helpers
// ---------------------------------------------------------------------------

function eventIcon(type: EventType) {
  switch (type) {
    case "agent_action": return <Bot className="h-3.5 w-3.5 text-violet-400" />;
    case "file_created": return <FilePlus className="h-3.5 w-3.5 text-emerald-400" />;
    case "file_updated": return <FileCode2 className="h-3.5 w-3.5 text-blue-400" />;
    case "component_ready": return <Layers className="h-3.5 w-3.5 text-amber-400" />;
    case "build_step": return <Terminal className="h-3.5 w-3.5 text-slate-400" />;
    case "progress": return <Activity className="h-3.5 w-3.5 text-sky-400" />;
    case "taste_score": return <Sparkles className="h-3.5 w-3.5 text-pink-400" />;
    case "complete": return <CheckCircle2 className="h-3.5 w-3.5 text-emerald-500" />;
    case "error": return <XCircle className="h-3.5 w-3.5 text-red-400" />;
    default: return <Zap className="h-3.5 w-3.5 text-slate-500" />;
  }
}

function eventLabel(event: BuildEvent): string {
  switch (event.type) {
    case "agent_action": return `[${event.agent}] ${event.action}${event.target ? ` -> ${event.target}` : ""}`;
    case "file_created": return `Created ${event.path} (${event.lines} lines, ${event.language})`;
    case "file_updated": return `Updated ${event.path}: ${event.change_summary}`;
    case "component_ready": return `Component ready: ${event.component} at ${event.path}`;
    case "build_step": return `${event.step} -- ${event.status}${event.duration_ms ? ` (${event.duration_ms}ms)` : ""}`;
    case "progress": return `${event.message} (${event.files_created} files${event.files_total ? `/${event.files_total}` : ""})`;
    case "taste_score": return `Taste score: ${event.overall}/100 -- ${event.message}`;
    case "complete": return `Build complete -- ${event.files_created} files in ${(event.duration_ms / 1000).toFixed(1)}s${event.taste_score ? ` / score ${event.taste_score}` : ""}`;
    case "error": return `Error: ${event.message}${event.recoverable ? " (recovering...)" : ""}`;
    case "lag": return "Stream lagged -- some events missed";
    default: return "Unknown event";
  }
}

function eventBadge(type: EventType) {
  const map: Record<string, string> = {
    agent_action: "bg-violet-500/10 text-violet-400 border-violet-500/20",
    file_created: "bg-emerald-500/10 text-emerald-400 border-emerald-500/20",
    file_updated: "bg-blue-500/10 text-blue-400 border-blue-500/20",
    component_ready: "bg-amber-500/10 text-amber-400 border-amber-500/20",
    build_step: "bg-slate-500/10 text-slate-400 border-slate-500/20",
    progress: "bg-sky-500/10 text-sky-400 border-sky-500/20",
    taste_score: "bg-pink-500/10 text-pink-400 border-pink-500/20",
    complete: "bg-emerald-600/10 text-emerald-400 border-emerald-600/20",
    error: "bg-red-500/10 text-red-400 border-red-500/20",
  };
  return map[type] ?? "bg-slate-500/10 text-slate-400 border-slate-500/20";
}

// ---------------------------------------------------------------------------
// ProjectBuild: combines Live Build + Debugger
// ---------------------------------------------------------------------------

export function ProjectBuild({ projectId }: { projectId: string }) {
  const [buildTab, setBuildTab] = useState("live");

  return (
    <div className="flex flex-col h-full">
      <Tabs value={buildTab} onValueChange={setBuildTab} className="flex-1 flex flex-col">
        <div className="px-6 pt-4">
          <TabsList className="bg-nexus-surface/60 border border-white/[0.06]">
            <TabsTrigger value="live" className="data-[state=active]:bg-glow-cyan/[0.08]">
              <Terminal className="h-4 w-4 mr-2" />Live Build
            </TabsTrigger>
            <TabsTrigger value="debugger" className="data-[state=active]:bg-glow-cyan/[0.08]">
              <Bug className="h-4 w-4 mr-2" />Debugger
            </TabsTrigger>
            <TabsTrigger value="preview" className="data-[state=active]:bg-glow-cyan/[0.08]">
              <Monitor className="h-4 w-4 mr-2" />Preview
            </TabsTrigger>
          </TabsList>
        </div>

        <TabsContent value="live" className="flex-1 mt-0 overflow-auto">
          <LiveBuildPanel projectId={projectId} />
        </TabsContent>

        <TabsContent value="debugger" className="flex-1 mt-0 overflow-auto">
          <DebuggerPanel projectId={projectId} />
        </TabsContent>

        <TabsContent value="preview" className="flex-1 mt-0 overflow-auto p-4">
          <LivePreviewPanel projectId={projectId} />
        </TabsContent>
      </Tabs>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Live Build Panel
// ---------------------------------------------------------------------------

function LiveBuildPanel({ projectId }: { projectId: string }) {
  const [log, setLog] = useState<LogEntry[]>([]);
  const [progress, setProgress] = useState(0);
  const [progressMsg, setProgressMsg] = useState("Waiting for build...");
  const [filesCreated, setFilesCreated] = useState(0);
  const [filesTotal, setFilesTotal] = useState<number | null>(null);
  const [tasteScore, setTasteScore] = useState<number | null>(null);
  const [isComplete, setIsComplete] = useState(false);
  const [isConnected, setIsConnected] = useState(false);
  const [activeAgent, setActiveAgent] = useState<string | null>(null);
  const [recentComponents, setRecentComponents] = useState<ComponentReadyEvent[]>([]);

  const logEndRef = useRef<HTMLDivElement>(null);
  const esRef = useRef<EventSource | null>(null);
  const counterRef = useRef(0);

  const appendEvent = useCallback((event: BuildEvent) => {
    const entry: LogEntry = { id: String(++counterRef.current), timestamp: new Date(), event };
    setLog((prev) => [...prev.slice(-199), entry]);
  }, []);

  useEffect(() => {
    if (!projectId) return;
    const url = `${BASE}/projects/${projectId}/live-build/stream`;
    const es = new EventSource(url);
    esRef.current = es;
    es.onopen = () => setIsConnected(true);

    const eventTypes: EventType[] = [
      "agent_action", "file_created", "file_updated", "component_ready",
      "build_step", "progress", "taste_score", "complete", "error", "lag",
    ];

    eventTypes.forEach((type) => {
      es.addEventListener(type, (e: MessageEvent) => {
        try {
          const data = JSON.parse(e.data) as BuildEvent;
          appendEvent(data);
          if (data.type === "progress") {
            setProgress(data.percent); setProgressMsg(data.message);
            setFilesCreated(data.files_created);
            if (data.files_total) setFilesTotal(data.files_total);
          } else if (data.type === "agent_action") { setActiveAgent(data.agent); }
          else if (data.type === "component_ready") { setRecentComponents((prev) => [data, ...prev].slice(0, 6)); }
          else if (data.type === "taste_score") { setTasteScore(data.overall); }
          else if (data.type === "complete") {
            setProgress(100); setProgressMsg("Build complete");
            setFilesCreated(data.files_created);
            if (data.taste_score) setTasteScore(data.taste_score);
            setIsComplete(true); setActiveAgent(null);
          } else if (data.type === "file_created") { setFilesCreated((n) => n + 1); }
        } catch { /* ignore parse errors */ }
      });
    });

    es.onerror = () => setIsConnected(false);
    return () => { es.close(); esRef.current = null; };
  }, [projectId, appendEvent]);

  useEffect(() => { logEndRef.current?.scrollIntoView({ behavior: "smooth" }); }, [log]);

  const tasteColor = tasteScore == null ? "text-slate-400" : tasteScore >= 80 ? "text-emerald-400" : tasteScore >= 60 ? "text-amber-400" : "text-red-400";

  return (
    <div className="flex flex-col gap-6 p-6">
      <div className="flex items-center gap-3 text-sm">
        <span className={cn("inline-flex items-center gap-1.5 rounded-full px-2.5 py-1 text-xs font-medium border", isConnected ? "bg-emerald-500/10 text-emerald-400 border-emerald-500/20" : "bg-slate-500/10 text-slate-400 border-slate-500/20")}>
          <span className={cn("h-1.5 w-1.5 rounded-full", isConnected ? "bg-emerald-400 animate-pulse" : "bg-slate-500")} />
          {isConnected ? "Connected" : "Waiting"}
        </span>
        {activeAgent && (
          <span className="inline-flex items-center gap-1.5 text-violet-400 text-xs">
            <Bot className="h-3.5 w-3.5" /><span className="font-medium">{activeAgent}</span> working...
          </span>
        )}
        {isComplete && (
          <span className="inline-flex items-center gap-1.5 text-emerald-400 text-xs font-medium">
            <CheckCircle2 className="h-3.5 w-3.5" />Build complete
          </span>
        )}
      </div>

      <Card className="border-white/[0.06] bg-nexus-surface/60">
        <CardContent className="pt-4 pb-4">
          <div className="flex items-center justify-between mb-2">
            <span className="text-sm text-slate-400">{progressMsg}</span>
            <span className="text-sm font-mono font-medium">{progress}%</span>
          </div>
          <Progress value={progress} className="h-1.5" />
          <div className="flex items-center gap-4 mt-3 text-xs text-slate-400">
            <span>{filesCreated} {filesTotal ? `/ ${filesTotal}` : ""} files</span>
            {tasteScore !== null && (
              <span className={cn("font-medium", tasteColor)}>
                <Sparkles className="h-3 w-3 inline mr-1" />Taste: {tasteScore}/100
              </span>
            )}
          </div>
        </CardContent>
      </Card>

      <div className="grid grid-cols-1 lg:grid-cols-3 gap-4">
        <div className="lg:col-span-2">
          <Card className="border-white/[0.06] bg-nexus-surface/60">
            <CardHeader className="pb-2 pt-3 px-4">
              <CardTitle className="text-sm font-medium flex items-center gap-2">
                <Terminal className="h-4 w-4 text-slate-400" />Build stream
                {log.length > 0 && <Badge variant="secondary" className="ml-auto text-xs">{log.length} events</Badge>}
              </CardTitle>
            </CardHeader>
            <CardContent className="px-0 pb-0">
              <div className="h-[480px] overflow-y-auto font-mono text-xs scrollbar-thin">
                {log.length === 0 ? (
                  <div className="flex flex-col items-center justify-center h-full gap-3 text-slate-400">
                    {isConnected ? (<><Loader2 className="h-5 w-5 animate-spin" /><span>Waiting for build events...</span></>) : (<><Activity className="h-5 w-5" /><span>Start a build to see live events here</span></>)}
                  </div>
                ) : (
                  <div className="divide-y divide-border/30">
                    {log.map((entry) => (
                      <div key={entry.id} className="flex items-start gap-2.5 px-4 py-2 hover:bg-white/[0.03] transition-colors">
                        <span className="text-slate-400/50 shrink-0 tabular-nums mt-0.5">
                          {entry.timestamp.toLocaleTimeString("en", { hour12: false, hour: "2-digit", minute: "2-digit", second: "2-digit" })}
                        </span>
                        <span className="shrink-0 mt-0.5">{eventIcon(entry.event.type as EventType)}</span>
                        <span className={cn("px-1.5 py-0.5 rounded border text-[10px] font-medium shrink-0", eventBadge(entry.event.type as EventType))}>{entry.event.type}</span>
                        <span className="text-slate-200/80 break-all">{eventLabel(entry.event)}</span>
                      </div>
                    ))}
                    <div ref={logEndRef} />
                  </div>
                )}
              </div>
            </CardContent>
          </Card>
        </div>

        <div className="flex flex-col gap-4">
          <Card className="border-white/[0.06] bg-nexus-surface/60">
            <CardHeader className="pb-2 pt-3 px-4">
              <CardTitle className="text-sm font-medium flex items-center gap-2"><Layers className="h-4 w-4 text-slate-400" />Components ready</CardTitle>
            </CardHeader>
            <CardContent className="px-4 pb-4">
              {recentComponents.length === 0 ? (
                <p className="text-xs text-slate-400 py-4 text-center">Components will appear here as they are generated</p>
              ) : (
                <div className="flex flex-col gap-2">
                  {recentComponents.map((c, i) => (
                    <div key={i} className="flex items-center gap-2 p-2 rounded-lg bg-white/[0.03] border border-white/[0.06]">
                      <Eye className="h-3.5 w-3.5 text-amber-400 shrink-0" />
                      <div className="min-w-0">
                        <p className="text-xs font-medium truncate">{c.component}</p>
                        <p className="text-[10px] text-slate-400 truncate">{c.path}</p>
                      </div>
                    </div>
                  ))}
                </div>
              )}
            </CardContent>
          </Card>

          <Card className="border-white/[0.06] bg-nexus-surface/60">
            <CardHeader className="pb-2 pt-3 px-4">
              <CardTitle className="text-sm font-medium flex items-center gap-2"><Zap className="h-4 w-4 text-slate-400" />Metrics</CardTitle>
            </CardHeader>
            <CardContent className="px-4 pb-4">
              <div className="grid grid-cols-2 gap-3">
                <div className="flex flex-col"><span className="text-xl font-bold tabular-nums">{filesCreated}</span><span className="text-[10px] text-slate-400">Files created</span></div>
                <div className="flex flex-col"><span className="text-xl font-bold tabular-nums">{log.filter((e) => e.event.type === "component_ready").length}</span><span className="text-[10px] text-slate-400">Components</span></div>
                <div className="flex flex-col"><span className="text-xl font-bold tabular-nums">{log.filter((e) => e.event.type === "agent_action").length}</span><span className="text-[10px] text-slate-400">Agent actions</span></div>
                <div className="flex flex-col"><span className={cn("text-xl font-bold tabular-nums", tasteColor)}>{tasteScore ?? "--"}</span><span className="text-[10px] text-slate-400">Taste score</span></div>
              </div>
            </CardContent>
          </Card>
        </div>
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Debugger Panel
// ---------------------------------------------------------------------------

function DebuggerPanel({ projectId }: { projectId: string }) {
  const [activeTab, setActiveTab] = useState("enforcement");
  const [loading, setLoading] = useState(false);
  const [enforcement, setEnforcement] = useState<EnforcementResult | null>(null);
  const [costs, setCosts] = useState<CostSummary | null>(null);
  const [estimate, setEstimate] = useState<PipelineEstimate | null>(null);
  const [gates, setGates] = useState<GateEntry[]>([]);
  const [traces, setTraces] = useState<TraceEntry[]>([]);
  const [healing, setHealing] = useState<HealingEvent[]>([]);

  const fetchAll = useCallback(async () => {
    if (!projectId) return;
    setLoading(true);
    try {
      const [enfRes, costRes, estRes, gateRes, traceRes, healRes] = await Promise.allSettled([
        api.enforceGate(projectId, "pre_deploy"),
        api.getCostSummary(),
        api.getSpeedEstimate(projectId),
        api.listGates(projectId),
        api.listTraces(projectId),
        api.listHealing(projectId),
      ]);
      if (enfRes.status === "fulfilled") setEnforcement(enfRes.value as unknown as EnforcementResult);
      if (costRes.status === "fulfilled") setCosts(costRes.value as unknown as CostSummary);
      if (estRes.status === "fulfilled") setEstimate(estRes.value as unknown as PipelineEstimate);
      if (gateRes.status === "fulfilled") setGates(gateRes.value as unknown as GateEntry[] ?? []);
      if (traceRes.status === "fulfilled") setTraces((traceRes.value as { traces?: TraceEntry[] })?.traces ?? []);
      if (healRes.status === "fulfilled") setHealing((healRes.value as { events?: HealingEvent[] })?.events ?? []);
    } catch { /* partial failures ok */ } finally { setLoading(false); }
  }, [projectId]);

  useEffect(() => { fetchAll(); }, [fetchAll]);

  return (
    <div className="flex flex-col h-full p-6">
      <div className="flex items-center justify-between mb-4">
        <div className="grid grid-cols-2 md:grid-cols-4 gap-4 flex-1">
          <KpiCard icon={Shield} label="Enforcement Score" value={enforcement ? `${enforcement.score}/100` : "--"} status={enforcement ? (enforcement.passed ? "success" : "danger") : "neutral"} />
          <KpiCard icon={DollarSign} label="Today's Cost" value={costs ? `$${costs.total_cost_usd.toFixed(2)}` : "--"} status="neutral" />
          <KpiCard icon={Zap} label="Est. Pipeline" value={estimate ? `${(estimate.total_estimated_ms / 1000).toFixed(1)}s` : "--"} status="neutral" />
          <KpiCard icon={Activity} label="Agent Traces" value={`${traces.length}`} status="neutral" />
        </div>
        <Button variant="outline" size="sm" onClick={fetchAll} disabled={loading} className="ml-4">
          {loading ? <Loader2 className="h-4 w-4 animate-spin mr-2" /> : <RefreshCw className="h-4 w-4 mr-2" />}Refresh
        </Button>
      </div>

      <Tabs value={activeTab} onValueChange={setActiveTab} className="flex-1 flex flex-col">
        <TabsList className="bg-nexus-surface/60 border border-white/[0.06]">
          <TabsTrigger value="enforcement" className="data-[state=active]:bg-glow-cyan/[0.08]"><Shield className="h-4 w-4 mr-2" />Enforcement</TabsTrigger>
          <TabsTrigger value="traces" className="data-[state=active]:bg-glow-cyan/[0.08]"><Brain className="h-4 w-4 mr-2" />Agent Traces</TabsTrigger>
          <TabsTrigger value="costs" className="data-[state=active]:bg-glow-cyan/[0.08]"><DollarSign className="h-4 w-4 mr-2" />Costs</TabsTrigger>
          <TabsTrigger value="pipeline" className="data-[state=active]:bg-glow-cyan/[0.08]"><Zap className="h-4 w-4 mr-2" />Pipeline</TabsTrigger>
          <TabsTrigger value="gates" className="data-[state=active]:bg-glow-cyan/[0.08]"><Eye className="h-4 w-4 mr-2" />Gates</TabsTrigger>
          <TabsTrigger value="healing" className="data-[state=active]:bg-glow-cyan/[0.08]"><Activity className="h-4 w-4 mr-2" />Self-Healing</TabsTrigger>
        </TabsList>

        <TabsContent value="enforcement" className="flex-1 mt-4 overflow-auto">
          {!enforcement ? <EmptyPanel message="No enforcement data. Run enforcement check first." /> : <EnforcementContent enforcement={enforcement} />}
        </TabsContent>
        <TabsContent value="traces" className="flex-1 mt-4 overflow-auto">
          {!traces.length ? <EmptyPanel message="No agent traces recorded yet." /> : <TracesContent traces={traces} />}
        </TabsContent>
        <TabsContent value="costs" className="flex-1 mt-4 overflow-auto">
          {!costs ? <EmptyPanel message="No cost data available." /> : <CostsContent costs={costs} />}
        </TabsContent>
        <TabsContent value="pipeline" className="flex-1 mt-4 overflow-auto">
          {!estimate ? <EmptyPanel message="No pipeline estimate available." /> : <PipelineContent estimate={estimate} />}
        </TabsContent>
        <TabsContent value="gates" className="flex-1 mt-4 overflow-auto">
          {!gates.length ? <EmptyPanel message="No approval gates recorded." /> : <GatesContent gates={gates} />}
        </TabsContent>
        <TabsContent value="healing" className="flex-1 mt-4 overflow-auto">
          {!healing.length ? <EmptyPanel message="No self-healing events recorded. The system is healthy!" /> : <HealingContent healing={healing} />}
        </TabsContent>
      </Tabs>
    </div>
  );
}

// Sub-components for debugger

function KpiCard({ icon: Icon, label, value, status }: { icon: React.ElementType; label: string; value: string; status: "success" | "danger" | "neutral" }) {
  const colors = { success: "text-green-400 bg-green-500/10 border-green-500/20", danger: "text-red-400 bg-red-500/10 border-red-500/20", neutral: "text-slate-400 bg-white/[0.03] border-white/[0.06]" };
  return (
    <Card className={cn("border", colors[status])}>
      <CardContent className="p-4 flex items-center gap-3">
        <Icon className={cn("h-8 w-8", status === "success" ? "text-green-400" : status === "danger" ? "text-red-400" : "text-slate-400")} />
        <div><p className="text-xs text-slate-400">{label}</p><p className="text-xl font-bold">{value}</p></div>
      </CardContent>
    </Card>
  );
}

function EmptyPanel({ message }: { message: string }) {
  return (
    <Card className="border border-white/[0.06]">
      <CardContent className="p-8 text-center text-slate-400">
        <Bug className="h-8 w-8 mx-auto mb-3 opacity-30" /><p>{message}</p>
      </CardContent>
    </Card>
  );
}

function EnforcementContent({ enforcement }: { enforcement: EnforcementResult }) {
  const categories = new Map<string, Violation[]>();
  for (const v of [...enforcement.blocking_violations, ...enforcement.warnings, ...enforcement.info]) {
    const cat = v.category ?? "unknown";
    if (!categories.has(cat)) categories.set(cat, []);
    categories.get(cat)!.push(v);
  }
  return (
    <div className="space-y-4">
      <Card className="border border-white/[0.06]">
        <CardContent className="p-4">
          <div className="flex justify-between items-center mb-2">
            <span className="text-sm font-medium">Production Readiness</span>
            <span className={cn("text-sm font-bold", enforcement.score >= 70 ? "text-green-400" : enforcement.score >= 50 ? "text-yellow-400" : "text-red-400")}>{enforcement.score}/100</span>
          </div>
          <Progress value={enforcement.score} className="h-2" />
          <div className="flex gap-4 mt-3 text-xs text-slate-400">
            <span className="flex items-center gap-1"><XCircle className="h-3 w-3 text-red-400" /> {enforcement.blocking_violations.length} blocking</span>
            <span className="flex items-center gap-1"><AlertTriangle className="h-3 w-3 text-yellow-400" /> {enforcement.warnings.length} warnings</span>
            <span className="flex items-center gap-1"><CheckCircle2 className="h-3 w-3 text-green-400" /> {enforcement.auto_fixed} auto-fixed</span>
          </div>
        </CardContent>
      </Card>
      {Array.from(categories.entries()).map(([cat, violations]) => (
        <ViolationGroup key={cat} category={cat} violations={violations} />
      ))}
    </div>
  );
}

function ViolationGroup({ category, violations }: { category: string; violations: Violation[] }) {
  const [expanded, setExpanded] = useState(violations.some(v => v.severity === "error"));
  return (
    <Card className="border border-white/[0.06]">
      <CardHeader className="p-3 cursor-pointer hover:bg-white/[0.02]" onClick={() => setExpanded(!expanded)}>
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-2">
            {expanded ? <ChevronDown className="h-4 w-4" /> : <ChevronRight className="h-4 w-4" />}
            <span className="font-medium capitalize">{category}</span><Badge variant="secondary">{violations.length}</Badge>
          </div>
          <div className="flex gap-1">
            {violations.some(v => v.severity === "error") && <Badge variant="destructive">errors</Badge>}
            {violations.some(v => v.severity === "warning") && <Badge variant="warning">warnings</Badge>}
          </div>
        </div>
      </CardHeader>
      {expanded && (
        <CardContent className="p-0">
          <div className="divide-y divide-border/30">
            {violations.map((v, i) => (
              <div key={i} className="px-4 py-2 text-sm flex items-start gap-3">
                {v.severity === "error" ? <XCircle className="h-4 w-4 text-red-400 mt-0.5 shrink-0" /> : v.severity === "warning" ? <AlertTriangle className="h-4 w-4 text-yellow-400 mt-0.5 shrink-0" /> : <CheckCircle2 className="h-4 w-4 text-slate-400 mt-0.5 shrink-0" />}
                <div className="flex-1 min-w-0">
                  <div className="flex items-center gap-2"><code className="text-xs bg-white/[0.03] px-1 rounded">{v.rule_id}</code><span className="text-slate-400 truncate">{v.file}{v.line ? `:${v.line}` : ""}</span></div>
                  <p className="text-slate-400 mt-0.5">{v.message}</p>
                  {v.fix && <p className="text-xs text-glow-cyan mt-1">Fix: {v.fix}</p>}
                </div>
              </div>
            ))}
          </div>
        </CardContent>
      )}
    </Card>
  );
}

function TracesContent({ traces }: { traces: TraceEntry[] }) {
  return (
    <div className="space-y-2">
      {traces.map((trace) => (
        <Card key={trace.id} className="border border-white/[0.06]">
          <CardContent className="p-4">
            <div className="flex items-center justify-between">
              <div className="flex items-center gap-3"><Brain className="h-5 w-5 text-glow-cyan" /><div><p className="font-medium">{trace.agent_name}</p><p className="text-sm text-slate-400">{trace.task_summary}</p></div></div>
              <div className="flex items-center gap-2">
                <Badge variant={trace.status === "completed" ? "success" : trace.status === "running" ? "warning" : "destructive"}>{trace.status}</Badge>
                {trace.total_tokens && <span className="text-xs text-slate-400">{(trace.total_tokens / 1000).toFixed(1)}K tokens</span>}
              </div>
            </div>
            <div className="flex gap-4 mt-2 text-xs text-slate-400">
              <span className="flex items-center gap-1"><Clock className="h-3 w-3" /> {new Date(trace.started_at).toLocaleTimeString()}</span>
              {trace.ended_at && <span>Duration: {((new Date(trace.ended_at).getTime() - new Date(trace.started_at).getTime()) / 1000).toFixed(1)}s</span>}
            </div>
          </CardContent>
        </Card>
      ))}
    </div>
  );
}

function CostsContent({ costs }: { costs: CostSummary }) {
  return (
    <div className="space-y-4">
      <div className="grid grid-cols-3 gap-4">
        <Card className="border border-white/[0.06]"><CardContent className="p-4 text-center"><p className="text-xs text-slate-400">Total Cost</p><p className="text-2xl font-bold text-glow-cyan">${costs.total_cost_usd.toFixed(4)}</p></CardContent></Card>
        <Card className="border border-white/[0.06]"><CardContent className="p-4 text-center"><p className="text-xs text-slate-400">Total Tokens</p><p className="text-2xl font-bold">{(costs.total_tokens / 1000).toFixed(1)}K</p></CardContent></Card>
        <Card className="border border-white/[0.06]"><CardContent className="p-4 text-center"><p className="text-xs text-slate-400">API Calls</p><p className="text-2xl font-bold">{costs.total_calls}</p></CardContent></Card>
      </div>
      {costs.records.length > 0 && (
        <Card className="border border-white/[0.06]">
          <CardHeader className="p-3"><CardTitle className="text-sm">Recent Calls</CardTitle></CardHeader>
          <CardContent className="p-0">
            <div className="divide-y divide-border/30 max-h-80 overflow-auto">
              {costs.records.slice(0, 20).map((r, i) => (
                <div key={i} className="px-4 py-2 text-sm flex justify-between items-center">
                  <div><code className="text-xs bg-white/[0.03] px-1 rounded">{r.model}</code><span className="ml-2 text-slate-400">{r.purpose}</span></div>
                  <div className="text-right"><span className="text-glow-cyan font-mono">${r.cost_usd.toFixed(4)}</span><span className="text-xs text-slate-400 ml-2">{(r.total_tokens / 1000).toFixed(1)}K</span></div>
                </div>
              ))}
            </div>
          </CardContent>
        </Card>
      )}
    </div>
  );
}

function PipelineContent({ estimate }: { estimate: PipelineEstimate }) {
  return (
    <div className="space-y-4">
      <Card className="border border-white/[0.06]">
        <CardContent className="p-4">
          <div className="flex justify-between items-center mb-3">
            <span className="text-sm font-medium">Estimated Pipeline Duration</span>
            <span className="text-lg font-bold text-glow-cyan">{(estimate.total_estimated_ms / 1000).toFixed(1)}s</span>
          </div>
          <div className="flex items-center gap-2 text-xs text-slate-400 mb-4"><span>Confidence: {(estimate.confidence * 100).toFixed(0)}%</span></div>
          <div className="space-y-2">
            {estimate.steps.map((step, i) => {
              const pct = (step.estimated_ms / estimate.total_estimated_ms) * 100;
              return (
                <div key={i} className="flex items-center gap-3">
                  <span className="text-xs text-slate-400 w-32 truncate">{step.name.replace(/_/g, " ")}</span>
                  <div className="flex-1 h-4 bg-white/[0.02] rounded overflow-hidden">
                    <div className={cn("h-full rounded", step.can_parallel ? "bg-blue-500/60" : "bg-glow-cyan/60")} style={{ width: `${Math.max(pct, 5)}%` }} />
                  </div>
                  <span className="text-xs font-mono w-16 text-right">{(step.estimated_ms / 1000).toFixed(1)}s</span>
                  <div className="flex gap-1 w-12">
                    {step.can_parallel && <GitBranch className="h-3 w-3 text-blue-400" aria-label="Parallelizable" />}
                    {step.skeleton_available && <FileCode className="h-3 w-3 text-green-400" aria-label="Skeleton available" />}
                  </div>
                </div>
              );
            })}
          </div>
        </CardContent>
      </Card>
    </div>
  );
}

function GatesContent({ gates }: { gates: GateEntry[] }) {
  return (
    <div className="space-y-2">
      {gates.map((gate) => (
        <Card key={gate.id} className="border border-white/[0.06]">
          <CardContent className="p-4 flex items-center justify-between">
            <div className="flex items-center gap-3">
              <Eye className={cn("h-5 w-5", gate.status === "pending" ? "text-yellow-400" : gate.status === "approved" || gate.status === "auto_approved" ? "text-green-400" : "text-red-400")} />
              <div>
                <p className="font-medium text-sm">{gate.description}</p>
                <div className="flex gap-2 mt-1"><Badge variant="secondary">{gate.gate_type}</Badge><Badge variant={gate.risk_level === "high" || gate.risk_level === "critical" ? "destructive" : "secondary"}>{gate.risk_level}</Badge></div>
              </div>
            </div>
            <Badge variant={gate.status === "approved" || gate.status === "auto_approved" ? "success" : gate.status === "pending" ? "warning" : "destructive"}>{gate.status}</Badge>
          </CardContent>
        </Card>
      ))}
    </div>
  );
}

function HealingContent({ healing }: { healing: HealingEvent[] }) {
  return (
    <div className="space-y-2">
      {healing.map((event) => (
        <Card key={event.id} className={cn("border", event.status === "healed" ? "border-green-500/20" : "border-red-500/20")}>
          <CardContent className="p-4">
            <div className="flex items-center justify-between mb-2">
              <div className="flex items-center gap-2"><Activity className={cn("h-5 w-5", event.status === "healed" ? "text-green-400" : "text-red-400")} /><span className="font-medium">Crash Recovery</span></div>
              <Badge variant={event.status === "healed" ? "success" : "destructive"}>{event.status}</Badge>
            </div>
            <pre className="text-xs bg-white/[0.02] rounded p-2 overflow-auto max-h-32 text-slate-400">{event.error_log.slice(0, 500)}</pre>
            {event.fix_summary && <p className="mt-2 text-sm text-green-400">{event.fix_summary}</p>}
            <p className="text-xs text-slate-400 mt-2">{new Date(event.created_at).toLocaleString()}</p>
          </CardContent>
        </Card>
      ))}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Live Preview Panel — iframe of the running generated app
// ---------------------------------------------------------------------------

function LivePreviewPanel({ projectId }: { projectId: string }) {
  const [appStatus, setAppStatus] = useState<{ port?: number; status?: string; url?: string | null } | null>(null);
  const [loading, setLoading] = useState(true);
  const [startError, setStartError] = useState<string | null>(null);

  useEffect(() => {
    let mounted = true;
    const fetchStatus = async () => {
      try {
        const status = await api.appStatus(projectId);
        if (mounted) {
          setAppStatus(status);
          setLoading(false);
        }
      } catch {
        if (mounted) setLoading(false);
      }
    };
    fetchStatus();
    const interval = setInterval(fetchStatus, 5000);
    return () => { mounted = false; clearInterval(interval); };
  }, [projectId]);

  if (loading) {
    return (
      <div className="flex items-center justify-center h-96">
        <Loader2 className="w-6 h-6 animate-spin text-slate-400" />
      </div>
    );
  }

  if (!appStatus || !appStatus.port) {
    return (
      <div className="flex flex-col items-center justify-center h-96 gap-4">
        <Monitor className="w-16 h-16 text-slate-400/30" />
        <p className="text-slate-400">No running app found. Generate and start your app first.</p>
        <Button
          variant="outline"
          onClick={async () => {
            setStartError(null);
            try {
              await api.startApp(projectId, true);
              const status = await api.appStatus(projectId);
              setAppStatus(status);
            } catch (err) {
              setStartError(err instanceof Error ? err.message : "Failed to start app");
            }
          }}
        >
          Start App
        </Button>
        {startError && (
          <p className="text-sm text-red-400 max-w-md text-center">{startError}</p>
        )}
      </div>
    );
  }

  return (
    <AppPreview
      port={appStatus.port}
      url={appStatus.url}
      status={appStatus.status || "stopped"}
      className="h-full"
    />
  );
}
