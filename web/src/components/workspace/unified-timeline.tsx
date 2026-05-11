"use client";

import { useState, useEffect, useRef } from "react";
import {
  Sparkles,
  Check,
  ChevronDown,
  ChevronRight,
  CheckCircle2,
  XCircle,
  Zap,
  FileText,
  FolderOpen,
  Search,
  Terminal,
  GitBranch,
  Pencil,
  Loader2,
  Database,
  Bot,
  Workflow,
  Globe,
  Layout,
  Plug,
  Rocket,
  Shield,
  TestTube,
  ClipboardList,
  Code2,
  AlertTriangle,
  Flag,
  X,
  Gauge,
  Palette,
  Container,
  Recycle,
  Lightbulb,
  Bug,
  Compass,
} from "lucide-react";
import { cn } from "@/lib/utils";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import type { TimelineItem } from "@/lib/unified-types";

// ---------------------------------------------------------------------------
// Thinking bubble
// ---------------------------------------------------------------------------

const THINKING_STEPS = [
  "Thinking...",
  "Reading between the lines...",
  "Considering the possibilities...",
  "Exploring creative paths...",
  "Connecting the dots...",
  "Building the solution...",
  "Structuring the answer...",
  "Reasoning through the logic...",
  "Brewing something good...",
  "Almost there...",
];

function pickRandom<T>(arr: T[], exclude?: T): T {
  const pool = exclude !== undefined ? arr.filter((x) => x !== exclude) : arr;
  return pool[Math.floor(Math.random() * pool.length)];
}

function ThinkingBubble() {
  const [step, setStep] = useState(() => pickRandom(THINKING_STEPS));
  const [visible, setVisible] = useState(true);

  useEffect(() => {
    let timeoutId: ReturnType<typeof setTimeout> | null = null;
    const interval = setInterval(() => {
      setVisible(false);
      timeoutId = setTimeout(() => {
        setStep((prev) => pickRandom(THINKING_STEPS, prev));
        setVisible(true);
      }, 250);
    }, 1800 + Math.random() * 800);
    return () => {
      clearInterval(interval);
      if (timeoutId) clearTimeout(timeoutId);
    };
  }, []);

  return (
    <div className="flex items-center gap-3 px-4 py-3 glass-card rounded-2xl w-fit">
      <div className="relative w-5 h-5 flex-shrink-0">
        <div className="absolute inset-0 rounded-full border-2 border-glow-cyan/30 border-t-primary animate-spin" />
      </div>
      <span
        className={cn(
          "text-[13px] text-slate-400 transition-opacity duration-250 min-w-[160px]",
          visible ? "opacity-100" : "opacity-0"
        )}
      >
        {step}
      </span>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Tool icons & helpers
// ---------------------------------------------------------------------------

const TOOL_ICONS: Record<string, typeof FileText> = {
  file_read: FileText,
  file_write: FileText,
  file_edit: Pencil,
  list_directory: FolderOpen,
  grep: Search,
  glob: Search,
  bash: Terminal,
  git_status: GitBranch,
  git_diff: GitBranch,
};

const TOOL_COLORS: Record<string, string> = {
  file_read: "text-blue-400",
  file_write: "text-emerald-400",
  file_edit: "text-amber-400",
  list_directory: "text-purple-400",
  grep: "text-cyan-400",
  glob: "text-cyan-400",
  bash: "text-orange-400",
  git_status: "text-pink-400",
  git_diff: "text-pink-400",
};

function summarizeArgs(tool: string, args: Record<string, unknown>): string {
  switch (tool) {
    case "file_read":
    case "file_write":
    case "file_edit":
      return String(args.path ?? "");
    case "grep":
      return `"${args.pattern}" ${args.include ? `in ${args.include}` : ""}`;
    case "glob":
      return String(args.pattern ?? "");
    case "bash":
      return String(args.command ?? "").slice(0, 80);
    case "list_directory":
      return String(args.path ?? ".");
    default:
      return JSON.stringify(args).slice(0, 60);
  }
}

const ACTION_ICONS: Record<string, React.ReactNode> = {
  create_table: <Database className="w-3.5 h-3.5" />,
  create_agent: <Bot className="w-3.5 h-3.5" />,
  create_workflow: <Workflow className="w-3.5 h-3.5" />,
  create_portal: <Globe className="w-3.5 h-3.5" />,
};

// ---------------------------------------------------------------------------
// Timeline entry renderers
// ---------------------------------------------------------------------------

function UserMessageEntry({ item }: { item: TimelineItem }) {
  return (
    <div className="flex justify-end">
      <div className="max-w-[70%] px-4 py-2.5 rounded-2xl bg-glow-cyan text-glow-cyan-foreground text-[14px] leading-relaxed shadow-[0_0_10px_rgba(249,115,22,0.1)]">
        {item.content}
      </div>
    </div>
  );
}

function AssistantMessageEntry({ item }: { item: TimelineItem }) {
  if (item.streaming && !item.content) {
    return <ThinkingBubble />;
  }

  return (
    <div className="max-w-[80%]">
      <div className="flex items-center gap-1.5 mb-1.5 ml-1">
        <div className="w-5 h-5 rounded-full bg-gradient-to-br from-glow-cyan to-glow-blue flex items-center justify-center shadow-glow-sm">
          <Sparkles className="w-3 h-3 text-white" />
        </div>
        <span className="text-[11px] text-slate-400 font-medium">Nexus</span>
        {item.phaseLabel && item.phaseNumber && item.phaseNumber > 1 && (
          <Badge
            variant={
              item.phaseNumber === 2 ? "info" : item.phaseNumber === 3 ? "purple" : item.phaseNumber === 4 ? "success" : "default"
            }
            className="text-[10px] ml-1"
          >
            {item.phaseLabel}
          </Badge>
        )}
      </div>
      <div className="glass-card px-4 py-3 rounded-2xl">
        <p className="text-[14px] text-slate-200 leading-relaxed whitespace-pre-wrap">
          {item.content}
          {item.streaming && (
            <span className="inline-block w-[3px] h-4 bg-glow-cyan rounded-sm ml-0.5 animate-pulse align-middle" />
          )}
        </p>
        {item.milestone && (
          <div className="mt-2 pt-2 border-t border-white/[0.06] text-[11px] text-slate-400 flex items-center gap-1">
            <Check className="w-3 h-3 text-emerald-400" />
            {item.milestone}
          </div>
        )}
      </div>
    </div>
  );
}

function ToolCallEntry({ item }: { item: TimelineItem }) {
  const [expanded, setExpanded] = useState(false);
  const Icon = TOOL_ICONS[item.toolName ?? ""] ?? Zap;
  const color = TOOL_COLORS[item.toolName ?? ""] ?? "text-glow-cyan";
  const pending = item.toolSuccess === undefined;

  return (
    <div
      className={cn(
        "rounded-xl border transition-all",
        item.toolSuccess === false
          ? "border-red-500/20 bg-red-500/[0.02]"
          : pending
          ? "border-violet-500/20 bg-violet-500/[0.02]"
          : "border-white/[0.08] bg-white/[0.02]"
      )}
    >
      <button
        onClick={() => setExpanded(!expanded)}
        className="w-full flex items-center gap-3 px-4 py-2.5 hover:bg-white/[0.02] transition-colors text-left"
      >
        <div className="w-7 h-7 rounded-lg flex items-center justify-center bg-white/[0.05] shrink-0">
          {pending ? (
            <Loader2 className={cn("w-3.5 h-3.5 animate-spin", color)} />
          ) : (
            <Icon className={cn("w-3.5 h-3.5", color)} />
          )}
        </div>
        <div className="flex-1 min-w-0">
          <div className="flex items-center gap-2">
            <span className="font-mono text-xs font-semibold">{item.toolName}</span>
            {item.toolSuccess !== undefined && (
              item.toolSuccess ? (
                <CheckCircle2 className="w-3.5 h-3.5 text-emerald-400" />
              ) : (
                <XCircle className="w-3.5 h-3.5 text-red-400" />
              )
            )}
            {item.toolDuration != null && (
              <span className="text-[10px] text-slate-400 font-mono">
                {item.toolDuration < 1000
                  ? `${item.toolDuration}ms`
                  : `${(item.toolDuration / 1000).toFixed(1)}s`}
              </span>
            )}
          </div>
          {item.toolArgs && (
            <p className="text-[11px] text-slate-400 truncate mt-0.5">
              {summarizeArgs(item.toolName ?? "", item.toolArgs)}
            </p>
          )}
        </div>
        {expanded ? (
          <ChevronDown className="w-4 h-4 text-slate-400 shrink-0" />
        ) : (
          <ChevronRight className="w-4 h-4 text-slate-400 shrink-0" />
        )}
      </button>
      {expanded && item.toolOutput && (
        <div className="border-t border-white/[0.06] px-4 py-3">
          <pre className="font-mono text-[11px] text-slate-400 whitespace-pre-wrap max-h-64 overflow-y-auto scrollbar-thin leading-relaxed">
            {item.toolOutput}
          </pre>
        </div>
      )}
    </div>
  );
}

function PlanBlockEntry({
  item,
  onApprove,
  onRegenerate,
  approving,
}: {
  item: TimelineItem;
  onApprove?: () => void;
  onRegenerate?: () => void;
  approving?: boolean;
}) {
  const [expanded, setExpanded] = useState(true);
  const plan = item.plan;
  if (!plan) return null;

  return (
    <div className="rounded-2xl border border-glow-cyan/20 bg-glow-cyan/[0.02] overflow-hidden">
      <button
        onClick={() => setExpanded(!expanded)}
        className="w-full flex items-center gap-3 px-4 py-3 hover:bg-white/[0.02] transition-colors text-left"
      >
        <div className="w-8 h-8 rounded-lg bg-glow-cyan/10 border border-glow-cyan/20 flex items-center justify-center shrink-0">
          <ClipboardList className="w-4 h-4 text-glow-cyan" />
        </div>
        <div className="flex-1 min-w-0">
          <span className="text-sm font-semibold text-slate-200">Project Plan</span>
          {plan.entities && (
            <span className="text-[11px] text-slate-400 ml-2">
              {plan.entities.length} entities
              {plan.agents ? `, ${plan.agents.length} agents` : ""}
              {plan.pages ? `, ${plan.pages.length} pages` : ""}
            </span>
          )}
        </div>
        {expanded ? (
          <ChevronDown className="w-4 h-4 text-slate-400 shrink-0" />
        ) : (
          <ChevronRight className="w-4 h-4 text-slate-400 shrink-0" />
        )}
      </button>

      {expanded && (
        <div className="border-t border-glow-cyan/10 px-4 py-3 space-y-3">
          {plan.summary && (
            <p className="text-sm text-slate-400 leading-relaxed">{plan.summary}</p>
          )}

          {/* Architecture */}
          {plan.architecture && (
            <div className="flex flex-wrap gap-1.5">
              {plan.architecture.framework && (
                <Badge variant="outline" className="gap-1 text-[10px]">
                  <Layout className="w-3 h-3" />
                  {plan.architecture.framework}
                </Badge>
              )}
              {plan.architecture.database && (
                <Badge variant="outline" className="gap-1 text-[10px]">
                  <Database className="w-3 h-3" />
                  {plan.architecture.database}
                </Badge>
              )}
              {plan.architecture.auth !== undefined && (
                <Badge variant={plan.architecture.auth ? "success" : "secondary"} className="gap-1 text-[10px]">
                  {plan.architecture.auth ? <Check className="w-3 h-3" /> : <X className="w-3 h-3" />}
                  Auth
                </Badge>
              )}
            </div>
          )}

          {/* Entities compact */}
          {plan.entities && plan.entities.length > 0 && (
            <div>
              <p className="text-[10px] font-medium text-slate-400 uppercase tracking-wider mb-1.5">
                Entities
              </p>
              <div className="flex flex-wrap gap-1.5">
                {plan.entities.map((e, idx) => (
                  <Badge key={`entity-${idx}-${e.name}`} variant="outline" className="text-[10px] gap-1">
                    <Database className="w-2.5 h-2.5" />
                    {e.name}
                    <span className="text-slate-400/60">({e.fields.length})</span>
                  </Badge>
                ))}
              </div>
            </div>
          )}

          {/* Agents compact */}
          {plan.agents && plan.agents.length > 0 && (
            <div>
              <p className="text-[10px] font-medium text-slate-400 uppercase tracking-wider mb-1.5">
                Agents
              </p>
              <div className="flex flex-wrap gap-1.5">
                {plan.agents.map((a, idx) => (
                  <Badge key={`agent-${idx}-${a.name}`} variant="outline" className="text-[10px] gap-1">
                    <Bot className="w-2.5 h-2.5" />
                    {a.name}
                  </Badge>
                ))}
              </div>
            </div>
          )}

          {/* Pages compact */}
          {plan.pages && plan.pages.length > 0 && (
            <div>
              <p className="text-[10px] font-medium text-slate-400 uppercase tracking-wider mb-1.5">
                Pages
              </p>
              <div className="flex flex-wrap gap-1.5">
                {plan.pages.map((p) => (
                  <Badge key={p.route} variant="outline" className="text-[10px] gap-1 font-mono">
                    <Layout className="w-2.5 h-2.5" />
                    {p.route}
                  </Badge>
                ))}
              </div>
            </div>
          )}

          {/* Integrations */}
          {plan.integrations && plan.integrations.length > 0 && (
            <div className="flex flex-wrap gap-1.5">
              {plan.integrations.map((i) => (
                <Badge key={i} variant="outline" className="text-[10px] gap-1">
                  <Plug className="w-2.5 h-2.5" />
                  {i}
                </Badge>
              ))}
            </div>
          )}

          {/* Actions */}
          {onApprove && (
            <div className="flex items-center gap-2 pt-2 border-t border-glow-cyan/10">
              <Button
                onClick={onApprove}
                disabled={approving}
                size="sm"
                className="gap-1.5 text-xs neon-glow"
              >
                {approving ? (
                  <Loader2 className="w-3.5 h-3.5 animate-spin" />
                ) : (
                  <Rocket className="w-3.5 h-3.5" />
                )}
                {approving ? "Building..." : "Approve & Build"}
              </Button>
              {onRegenerate && (
                <Button variant="outline" size="sm" className="gap-1.5 text-xs" onClick={onRegenerate}>
                  <Sparkles className="w-3.5 h-3.5" />
                  Regenerate
                </Button>
              )}
            </div>
          )}
        </div>
      )}
    </div>
  );
}

function ActionResultEntry({ item }: { item: TimelineItem }) {
  const icon = ACTION_ICONS[item.action ?? ""];
  const labels: Record<string, string> = {
    create_table: `Table created: ${item.actionData?.table_name ?? ""}`,
    create_agent: `Agent ready: ${item.actionData?.agent_name ?? ""}`,
    create_workflow: `Workflow set up: ${item.actionData?.name ?? ""}`,
    create_portal: `Portal built: /${item.actionData?.slug ?? ""}`,
  };

  return (
    <div className="flex items-center gap-2 px-3 py-2 rounded-lg bg-emerald-500/10 border border-emerald-500/20 text-[12px] text-emerald-400 w-fit">
      {icon ?? <Check className="w-3.5 h-3.5" />}
      <span>{labels[item.action ?? ""] ?? item.action}</span>
    </div>
  );
}

const AGENT_PHASE_CONFIG: Record<string, { icon: typeof Zap; color: string; bgColor: string; borderColor: string; label: string }> = {
  architect:    { icon: Compass,       color: "text-violet-400",  bgColor: "bg-violet-500/[0.05]",  borderColor: "border-violet-500/20",  label: "Architect" },
  plan:         { icon: Compass,       color: "text-violet-400",  bgColor: "bg-violet-500/[0.05]",  borderColor: "border-violet-500/20",  label: "Architect" },
  coder:        { icon: Code2,         color: "text-blue-400",    bgColor: "bg-blue-500/[0.05]",    borderColor: "border-blue-500/20",    label: "Coder" },
  implement:    { icon: Code2,         color: "text-blue-400",    bgColor: "bg-blue-500/[0.05]",    borderColor: "border-blue-500/20",    label: "Coder" },
  reviewer:     { icon: Shield,        color: "text-amber-400",   bgColor: "bg-amber-500/[0.05]",   borderColor: "border-amber-500/20",   label: "Reviewer" },
  review:       { icon: Shield,        color: "text-amber-400",   bgColor: "bg-amber-500/[0.05]",   borderColor: "border-amber-500/20",   label: "Reviewer" },
  tester:       { icon: TestTube,      color: "text-green-400",   bgColor: "bg-green-500/[0.05]",   borderColor: "border-green-500/20",   label: "Tester" },
  test:         { icon: TestTube,      color: "text-green-400",   bgColor: "bg-green-500/[0.05]",   borderColor: "border-green-500/20",   label: "Tester" },
  debugger:     { icon: Bug,           color: "text-red-400",     bgColor: "bg-red-500/[0.05]",     borderColor: "border-red-500/20",     label: "Debugger" },
  debug:        { icon: Bug,           color: "text-red-400",     bgColor: "bg-red-500/[0.05]",     borderColor: "border-red-500/20",     label: "Debugger" },
  performance:  { icon: Gauge,         color: "text-orange-400",  bgColor: "bg-orange-500/[0.05]",  borderColor: "border-orange-500/20",  label: "Performance" },
  optimize:     { icon: Gauge,         color: "text-orange-400",  bgColor: "bg-orange-500/[0.05]",  borderColor: "border-orange-500/20",  label: "Performance" },
  ux:           { icon: Palette,       color: "text-pink-400",    bgColor: "bg-pink-500/[0.05]",    borderColor: "border-pink-500/20",    label: "UX" },
  devops:       { icon: Container,     color: "text-cyan-400",    bgColor: "bg-cyan-500/[0.05]",    borderColor: "border-cyan-500/20",    label: "DevOps" },
  deploy:       { icon: Container,     color: "text-cyan-400",    bgColor: "bg-cyan-500/[0.05]",    borderColor: "border-cyan-500/20",    label: "DevOps" },
  refactor:     { icon: Recycle,       color: "text-teal-400",    bgColor: "bg-teal-500/[0.05]",    borderColor: "border-teal-500/20",    label: "Refactor" },
  product:      { icon: Lightbulb,     color: "text-yellow-400",  bgColor: "bg-yellow-500/[0.05]",  borderColor: "border-yellow-500/20",  label: "Product" },
  verify:       { icon: CheckCircle2,  color: "text-emerald-400", bgColor: "bg-emerald-500/[0.05]", borderColor: "border-emerald-500/20", label: "Verify" },
};

const DEFAULT_PHASE_CONFIG = { icon: Zap, color: "text-violet-400", bgColor: "bg-violet-500/[0.05]", borderColor: "border-violet-500/20", label: "Agent" };

function PhaseEntry({ item }: { item: TimelineItem }) {
  const phaseName = item.agentRole ?? item.phaseName ?? "";
  const config = AGENT_PHASE_CONFIG[phaseName] ?? DEFAULT_PHASE_CONFIG;
  const PhaseIcon = config.icon;
  const isStart = item.phaseStatus === "running" || !item.phaseStatus;

  return (
    <div
      className={cn(
        "flex items-center gap-3 px-4 py-2.5 rounded-xl border",
        isStart ? `${config.borderColor} ${config.bgColor}` : "border-emerald-500/20 bg-emerald-500/[0.03]"
      )}
    >
      {isStart ? (
        <Loader2 className={cn("w-4 h-4 animate-spin", config.color)} />
      ) : (
        <CheckCircle2 className="w-4 h-4 text-emerald-400" />
      )}
      <PhaseIcon className={cn("w-4 h-4", isStart ? config.color : "text-emerald-400")} />
      <span className={cn("text-sm font-medium", isStart ? config.color : "text-emerald-400")}>
        {config.label} {isStart ? "starting..." : "completed"}
      </span>
    </div>
  );
}

function DoneEntry({ item }: { item: TimelineItem }) {
  return (
    <div className="rounded-xl border border-emerald-500/20 bg-emerald-500/[0.03] p-4">
      <div className="flex items-center gap-2 mb-2">
        <CheckCircle2 className="w-5 h-5 text-emerald-400" />
        <span className="font-semibold text-sm text-emerald-400">Task Complete</span>
        {item.totalIterations && (
          <Badge variant="secondary" className="text-[10px]">
            {item.totalIterations} iterations
          </Badge>
        )}
      </div>
      {item.totalTools && item.totalTools.length > 0 && (
        <div className="flex flex-wrap gap-1">
          {[...new Set(item.totalTools)].map((t) => {
            const Icon = TOOL_ICONS[t] ?? Zap;
            return (
              <Badge key={t} variant="outline" className="text-[10px] gap-1">
                <Icon className="w-2.5 h-2.5" />
                {t} ({item.totalTools!.filter((x) => x === t).length})
              </Badge>
            );
          })}
        </div>
      )}
    </div>
  );
}

function ErrorEntry({ item }: { item: TimelineItem }) {
  return (
    <div className="rounded-xl border border-red-500/20 bg-red-500/[0.03] p-4">
      <div className="flex items-center gap-2 mb-1">
        <XCircle className="w-4 h-4 text-red-400" />
        <span className="font-semibold text-sm text-red-400">Error</span>
      </div>
      <p className="text-xs text-red-400/80">{item.errorMessage}</p>
    </div>
  );
}

function MilestoneEntry({ item }: { item: TimelineItem }) {
  return (
    <div className="flex items-center gap-3 px-4 py-3 rounded-xl border border-emerald-500/20 bg-emerald-500/[0.03]">
      <Flag className="w-4 h-4 text-emerald-400" />
      <span className="text-sm font-medium text-emerald-400">{item.milestone}</span>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Thinking entry — collapsible agent internal reasoning
// ---------------------------------------------------------------------------

function ThinkingEntry({ item }: { item: TimelineItem }) {
  const [expanded, setExpanded] = useState(false);
  if (!item.content) return null;

  return (
    <div className="rounded-xl border border-violet-500/10 bg-violet-500/[0.02] overflow-hidden">
      <button
        onClick={() => setExpanded(!expanded)}
        className="w-full flex items-center gap-2.5 px-4 py-2 hover:bg-white/[0.02] transition-colors text-left"
      >
        <div className="w-5 h-5 rounded-full bg-violet-500/10 flex items-center justify-center shrink-0">
          <Sparkles className="w-3 h-3 text-violet-400" />
        </div>
        <span className="text-[11px] text-violet-400 font-medium">Agent reasoning</span>
        <span className="text-[10px] text-slate-400 ml-auto">
          {item.content.length > 100 ? `${item.content.length} chars` : ""}
        </span>
        {expanded ? (
          <ChevronDown className="w-3.5 h-3.5 text-slate-400 shrink-0" />
        ) : (
          <ChevronRight className="w-3.5 h-3.5 text-slate-400 shrink-0" />
        )}
      </button>
      {expanded && (
        <div className="border-t border-violet-500/10 px-4 py-3">
          <p className="text-[12px] text-slate-400 leading-relaxed whitespace-pre-wrap">
            {item.content}
          </p>
        </div>
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Proposal entry — HITL change proposal from agent
// ---------------------------------------------------------------------------

function ProposalEntry({ item }: { item: TimelineItem }) {
  const [expanded, setExpanded] = useState(true);
  const riskColors: Record<string, string> = {
    low: "text-emerald-400 border-emerald-500/20 bg-emerald-500/[0.03]",
    medium: "text-amber-400 border-amber-500/20 bg-amber-500/[0.03]",
    high: "text-red-400 border-red-500/20 bg-red-500/[0.03]",
  };
  const riskClass = riskColors[item.proposalRiskLevel ?? "medium"] ?? riskColors.medium;

  return (
    <div className={cn("rounded-xl border overflow-hidden", riskClass)}>
      <button
        onClick={() => setExpanded(!expanded)}
        className="w-full flex items-center gap-3 px-4 py-3 hover:bg-white/[0.02] transition-colors text-left"
      >
        <AlertTriangle className="w-4 h-4 shrink-0" />
        <div className="flex-1 min-w-0">
          <span className="text-sm font-semibold">Proposed Changes</span>
          {item.proposalRiskLevel && (
            <Badge variant="outline" className="text-[9px] ml-2 capitalize">
              {item.proposalRiskLevel} risk
            </Badge>
          )}
        </div>
        {expanded ? (
          <ChevronDown className="w-4 h-4 text-slate-400 shrink-0" />
        ) : (
          <ChevronRight className="w-4 h-4 text-slate-400 shrink-0" />
        )}
      </button>
      {expanded && (
        <div className="border-t border-inherit px-4 py-3 space-y-2">
          {item.proposalIntent && (
            <p className="text-sm text-slate-200">{item.proposalIntent}</p>
          )}
          {item.proposalOperations && item.proposalOperations.length > 0 && (
            <div className="space-y-1">
              {item.proposalOperations.map((op, i) => (
                <div key={i} className="flex items-center gap-2 text-[11px]">
                  <FileText className="w-3 h-3 shrink-0" />
                  <span className="font-mono text-slate-400">
                    {op.file ?? op.description ?? op.action}
                  </span>
                  {op.action && (
                    <Badge variant="outline" className="text-[9px] px-1 py-0">
                      {op.action}
                    </Badge>
                  )}
                </div>
              ))}
            </div>
          )}
          {item.proposalWarnings && item.proposalWarnings.length > 0 && (
            <div className="space-y-1">
              {item.proposalWarnings.map((w, i) => (
                <div key={i} className="flex items-center gap-2 text-[11px] text-amber-400">
                  <AlertTriangle className="w-3 h-3 shrink-0" />
                  <span>{w}</span>
                </div>
              ))}
            </div>
          )}
        </div>
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Main timeline component
// ---------------------------------------------------------------------------

interface UnifiedTimelineProps {
  items: TimelineItem[];
  streaming: boolean;
  agentRunning: boolean;
  processingSteps: { action: string; data: Record<string, unknown>; done: boolean }[];
  onApprovePlan?: () => void;
  onRegeneratePlan?: () => void;
  approving?: boolean;
  onSuggestionClick?: (text: string) => void;
}

export function UnifiedTimeline({
  items,
  streaming: _streaming,
  agentRunning,
  processingSteps,
  onApprovePlan,
  onRegeneratePlan,
  approving,
  onSuggestionClick,
}: UnifiedTimelineProps) {
  const bottomRef = useRef<HTMLDivElement>(null);
  const scrollContainerRef = useRef<HTMLDivElement>(null);
  const userScrolledUp = useRef(false);

  // Smart auto-scroll: only auto-scroll if user hasn't scrolled up
  useEffect(() => {
    if (!userScrolledUp.current) {
      bottomRef.current?.scrollIntoView({ behavior: "smooth" });
    }
  }, [items, processingSteps]);

  // Detect user scrolling up
  useEffect(() => {
    const container = scrollContainerRef.current;
    if (!container) return;
    const handleScroll = () => {
      const { scrollTop, scrollHeight, clientHeight } = container;
      userScrolledUp.current = scrollHeight - scrollTop - clientHeight > 100;
    };
    container.addEventListener("scroll", handleScroll, { passive: true });
    return () => container.removeEventListener("scroll", handleScroll);
  }, []);

  return (
    <div ref={scrollContainerRef} className="flex-1 overflow-y-auto scrollbar-thin px-6 py-6 dot-grid">
      <div className="max-w-3xl mx-auto space-y-3">
        {items.length === 0 && (
          <EmptyState onSuggestionClick={onSuggestionClick} />
        )}
        {items.map((item) => {
          switch (item.type) {
            case "user_message":
              return <UserMessageEntry key={item.id} item={item} />;
            case "assistant_message":
              return <AssistantMessageEntry key={item.id} item={item} />;
            case "thinking":
              return <ThinkingEntry key={item.id} item={item} />;
            case "plan_block":
              return (
                <PlanBlockEntry
                  key={item.id}
                  item={item}
                  onApprove={onApprovePlan}
                  onRegenerate={onRegeneratePlan}
                  approving={approving}
                />
              );
            case "agent_tool":
              return <ToolCallEntry key={item.id} item={item} />;
            case "agent_phase":
              return <PhaseEntry key={item.id} item={item} />;
            case "agent_done":
              return <DoneEntry key={item.id} item={item} />;
            case "agent_proposal":
              return <ProposalEntry key={item.id} item={item} />;
            case "action_result":
              return <ActionResultEntry key={item.id} item={item} />;
            case "error":
              return <ErrorEntry key={item.id} item={item} />;
            case "milestone":
              return <MilestoneEntry key={item.id} item={item} />;
            default:
              return null;
          }
        })}

        {/* Loading indicator for agent */}
        {agentRunning && !items.some((i) => i.streaming) && (
          <div className="flex items-center gap-2 text-sm text-slate-400">
            <Loader2 className="w-4 h-4 animate-spin text-violet-400" />
            <span>Agent working...</span>
          </div>
        )}

        <div ref={bottomRef} />
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Empty state
// ---------------------------------------------------------------------------

function EmptyState({ onSuggestionClick }: { onSuggestionClick?: (text: string) => void }) {
  const suggestions = [
    "Build a wine marketplace with AI sommelier",
    "Create a SaaS project management tool",
    "Design a real-time analytics dashboard",
    "Build an AI-powered customer support system",
  ];

  return (
    <div className="flex flex-col items-center justify-center min-h-[60vh]">
      <div className="w-20 h-20 rounded-2xl bg-gradient-to-br from-primary/10 to-violet-500/10 border border-glow-cyan/10 flex items-center justify-center mb-6">
        <Sparkles className="w-10 h-10 text-glow-cyan/50" />
      </div>
      <h2 className="text-lg font-semibold mb-2">What would you like to build?</h2>
      <p className="text-sm text-slate-400 text-center max-w-md mb-6">
        Describe your idea. Nexus will plan the architecture, execute the build,
        and explain every decision — all in this single workspace.
      </p>
      <div className="grid grid-cols-1 sm:grid-cols-2 gap-3 max-w-lg w-full">
        {suggestions.map((suggestion) => (
          <button
            key={suggestion}
            onClick={() => onSuggestionClick?.(suggestion)}
            className="px-4 py-3 rounded-xl border border-white/[0.08] bg-white/[0.02] hover:bg-white/[0.05] hover:border-glow-cyan/20 text-xs text-slate-400 hover:text-slate-200 transition-all text-left group"
          >
            <span className="group-hover:text-glow-cyan transition-colors">{suggestion}</span>
          </button>
        ))}
      </div>
    </div>
  );
}
