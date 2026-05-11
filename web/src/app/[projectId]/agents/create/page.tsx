"use client";

import { useState, useCallback } from "react";
import { useParams, useRouter } from "next/navigation";
import {
  ArrowLeft,
  ArrowRight,
  Bot,
  Check,
  ChevronRight,
  Loader2,
  Pencil,
  Play,
  Plus,
  Rocket,
  Sparkles,
  Trash2,
  Wand2,
  Wrench,
  Zap,
  Network,
  GitBranch,
  Layers,
  Radio,
  X,
} from "lucide-react";
import { cn } from "@/lib/utils";
import {
  api,
  type DesignedWorkflow,
  type DesignedWorkflowAgent,
} from "@/lib/api";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { GlowButton } from "@/components/ui/glow-button";
import { GlowCard } from "@/components/ui/glow-card";
import { Input } from "@/components/ui/input";
import { Separator } from "@/components/ui/separator";

/* ------------------------------------------------------------------ */
/*  Constants                                                          */
/* ------------------------------------------------------------------ */

const STEPS = [
  { id: "idea", label: "Describe", icon: Wand2 },
  { id: "design", label: "Design", icon: Sparkles },
  { id: "configure", label: "Refine", icon: Wrench },
  { id: "deploy", label: "Hire", icon: Rocket },
] as const;

type StepId = (typeof STEPS)[number]["id"];

interface Blueprint {
  label: string;
  idea: string;
  icon: string;
  headcount: string;
  tags: string[];
}

const COMPANY_BLUEPRINTS: Blueprint[] = [
  {
    label: "SaaS Startup",
    icon: "🚀",
    headcount: "6–8 employees",
    tags: ["product", "engineering", "go-to-market"],
    idea: "Build an AI SaaS startup team with: a CEO who sets strategy and unblocks the team, a Product Manager who writes requirements, an Engineering Lead, a Frontend Engineer, a Backend Engineer, a Designer, a Growth Marketer, and a Customer Support Agent. They should collaborate: PM drafts specs → Engineers implement → Designer reviews UX → Marketer writes launch copy → Support handles incoming users.",
  },
  {
    label: "AI Agency",
    icon: "🎨",
    headcount: "5–7 employees",
    tags: ["clients", "creative", "delivery"],
    idea: "Build an AI creative agency team with: an Account Manager who handles client briefs, a Creative Director who sets vision, two Writers (long-form + social), a Designer, a Data Analyst for reporting, and a Project Manager who keeps delivery on track.",
  },
  {
    label: "Customer Support Org",
    icon: "🎧",
    headcount: "4–6 employees",
    tags: ["tiered", "escalation", "knowledge"],
    idea: "Build a tiered customer support organization with: a Tier 1 Triage Agent that classifies tickets, Tier 2 Specialist Agents (billing, technical, onboarding), a Tier 3 Escalation Manager for complex cases, a Knowledge Base Writer who turns resolved tickets into articles, and a QA Agent that audits responses for tone and accuracy.",
  },
  {
    label: "Research Lab",
    icon: "🔬",
    headcount: "4–5 employees",
    tags: ["research", "synthesis", "publication"],
    idea: "Build a research lab team with: a Principal Investigator who sets the research agenda, a Literature Researcher who mines papers and prior art, a Data Engineer who runs experiments, a Technical Writer who produces publications, and a Peer Reviewer who critiques findings.",
  },
  {
    label: "DevOps Team",
    icon: "⚙️",
    headcount: "5 employees",
    tags: ["sre", "security", "oncall"],
    idea: "Build a DevOps / SRE team with: an SRE Lead who owns reliability targets, an Infrastructure Engineer for IaC and cloud, a Security Auditor (OWASP + hardening), a Monitoring Specialist who builds dashboards and alerts, and an Incident Commander who coordinates on-call response and writes post-mortems.",
  },
  {
    label: "Content Studio",
    icon: "✍️",
    headcount: "5–6 employees",
    tags: ["research", "writing", "distribution"],
    idea: "Build a content studio team with: an Editor-in-Chief who owns the editorial calendar, a Researcher who finds trending topics, two Writers (long-form articles + social threads), an SEO Specialist, and a Distribution Agent that schedules and cross-posts across platforms.",
  },
  {
    label: "E-commerce Operations",
    icon: "🛒",
    headcount: "5–7 employees",
    tags: ["merchandising", "fulfillment", "growth"],
    idea: "Build an e-commerce operations team with: a Merchandiser who manages catalog and pricing, an Inventory Analyst, a Marketing Campaign Manager, a Customer Support Agent for order issues, a Fraud / Returns specialist, and a Growth Analyst tracking LTV and CAC.",
  },
  {
    label: "Sales Organization",
    icon: "🎯",
    headcount: "5 employees",
    tags: ["pipeline", "qualification", "closing"],
    idea: "Build a sales organization with: an SDR that prospects and qualifies leads, an Account Executive who runs discovery and closes deals, a Sales Engineer for technical demos, a Customer Success Manager for expansion and renewals, and a RevOps Analyst who reports on pipeline health.",
  },
];

const EXECUTION_MODE_CONFIG: Record<
  string,
  { icon: typeof Layers; label: string; color: string }
> = {
  sequential: { icon: ChevronRight, label: "Sequential", color: "text-blue-400" },
  parallel: { icon: Layers, label: "Parallel", color: "text-emerald-400" },
  pipeline: { icon: GitBranch, label: "Pipeline", color: "text-purple-400" },
  event_driven: { icon: Radio, label: "Event-Driven", color: "text-amber-400" },
};

const COMPLEXITY_COLORS: Record<string, string> = {
  simple: "text-emerald-400 bg-emerald-500/10 border-emerald-500/20",
  moderate: "text-amber-400 bg-amber-500/10 border-amber-500/20",
  complex: "text-red-400 bg-red-500/10 border-red-500/20",
};

/* ------------------------------------------------------------------ */
/*  Step Indicator                                                     */
/* ------------------------------------------------------------------ */

function StepIndicator({
  currentStep,
  completedSteps,
}: {
  currentStep: StepId;
  completedSteps: Set<StepId>;
}) {
  const currentIdx = STEPS.findIndex((s) => s.id === currentStep);

  return (
    <div className="flex items-center gap-1">
      {STEPS.map((step, i) => {
        const isActive = step.id === currentStep;
        const isCompleted = completedSteps.has(step.id);
        const isPast = i < currentIdx;
        const Icon = step.icon;

        return (
          <div key={step.id} className="flex items-center">
            <div
              className={cn(
                "flex items-center gap-2 px-3 py-1.5 rounded-lg transition-all duration-300",
                isActive && "bg-white/[0.08] border border-white/[0.15]",
                !isActive && "opacity-50"
              )}
            >
              <div
                className={cn(
                  "w-6 h-6 rounded-full flex items-center justify-center text-xs font-bold transition-all duration-300",
                  isCompleted || isPast
                    ? "bg-emerald-500/20 text-emerald-400"
                    : isActive
                      ? "bg-glow-cyan/20 text-glow-cyan"
                      : "bg-white/[0.05] text-slate-500"
                )}
              >
                {isCompleted || isPast ? (
                  <Check className="w-3 h-3" />
                ) : (
                  <Icon className="w-3 h-3" />
                )}
              </div>
              <span
                className={cn(
                  "text-xs font-medium hidden sm:block",
                  isActive ? "text-white" : "text-slate-400"
                )}
              >
                {step.label}
              </span>
            </div>
            {i < STEPS.length - 1 && (
              <div
                className={cn(
                  "w-8 h-px mx-1",
                  i < currentIdx ? "bg-emerald-500/40" : "bg-white/[0.06]"
                )}
              />
            )}
          </div>
        );
      })}
    </div>
  );
}

/* ------------------------------------------------------------------ */
/*  Step 1: Idea Input                                                 */
/* ------------------------------------------------------------------ */

function IdeaStep({
  idea,
  setIdea,
  onNext,
}: {
  idea: string;
  setIdea: (v: string) => void;
  onNext: () => void;
}) {
  return (
    <div className="max-w-5xl mx-auto space-y-8 animate-in fade-in slide-in-from-bottom-4 duration-500">
      <div className="text-center space-y-3">
        <div className="w-16 h-16 rounded-2xl bg-gradient-to-br from-glow-cyan/20 to-glow-purple/20 border border-white/[0.1] flex items-center justify-center mx-auto">
          <Wand2 className="w-8 h-8 text-glow-cyan" />
        </div>
        <h2 className="text-3xl font-bold tracking-tight">Build Your AI Company</h2>
        <p className="text-slate-400 max-w-2xl mx-auto leading-relaxed">
          Describe the company, team, or workflow you want — in plain English.
          Nexus designs the org chart, writes every role, and deploys your AI
          employees so they can start working together.
        </p>
      </div>

      <div className="max-w-3xl mx-auto">
        <label className="text-[11px] font-medium text-slate-400 uppercase tracking-wider mb-2 block">
          Describe your AI company
        </label>
        <div className="relative">
          <textarea
            value={idea}
            onChange={(e) => setIdea(e.target.value)}
            placeholder="e.g. Build a B2B SaaS startup team — a CEO, a PM, two engineers, a designer, a growth marketer, and a customer support agent. They ship a new feature every two weeks and handle incoming users autonomously."
            rows={5}
            className={cn(
              "w-full bg-white/[0.03] border rounded-xl p-5 text-base text-white placeholder:text-slate-500",
              "resize-none focus:outline-none transition-all duration-300",
              "border-white/[0.08] focus:border-glow-cyan/30 focus:shadow-[0_0_30px_rgba(0,200,255,0.08)]",
              "font-normal leading-relaxed"
            )}
            autoFocus
            onKeyDown={(e) => {
              if (e.key === "Enter" && (e.metaKey || e.ctrlKey) && idea.trim().length > 10) {
                onNext();
              }
            }}
          />
          <div className="absolute bottom-3 right-3 flex items-center gap-2">
            <span className="text-[10px] text-slate-500">
              {idea.length > 0 ? `${idea.length} chars` : ""}
            </span>
            {idea.trim().length > 10 && (
              <kbd className="text-[10px] text-slate-500 bg-white/[0.05] px-1.5 py-0.5 rounded border border-white/[0.08]">
                Cmd+Enter
              </kbd>
            )}
          </div>
        </div>
      </div>

      <div>
        <div className="flex items-center justify-between mb-3 max-w-5xl mx-auto">
          <p className="text-xs font-medium text-slate-400 uppercase tracking-wider">
            Or hire a proven team — start from a company blueprint
          </p>
          <span className="text-[10px] text-slate-500">
            {COMPANY_BLUEPRINTS.length} blueprints
          </span>
        </div>
        <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-3">
          {COMPANY_BLUEPRINTS.map((bp) => (
            <button
              key={bp.label}
              onClick={() => setIdea(bp.idea)}
              className={cn(
                "text-left p-4 rounded-xl border transition-all duration-200 group",
                idea === bp.idea
                  ? "border-glow-cyan/30 bg-glow-cyan/[0.05] shadow-[0_0_20px_rgba(0,200,255,0.06)]"
                  : "border-white/[0.06] bg-white/[0.02] hover:border-white/[0.12] hover:bg-white/[0.04]"
              )}
            >
              <div className="flex items-start justify-between gap-2 mb-2">
                <div className="flex items-center gap-2 min-w-0">
                  <span className="text-2xl shrink-0">{bp.icon}</span>
                  <span className="text-sm font-semibold truncate group-hover:text-white transition-colors">
                    {bp.label}
                  </span>
                </div>
              </div>
              <p className="text-[11px] text-slate-400/80 mb-3 line-clamp-2 leading-relaxed">
                {bp.idea.split(":")[1]?.split(".")[0] ?? bp.idea.slice(0, 120)}
              </p>
              <div className="flex items-center gap-1.5 flex-wrap">
                <span className="text-[10px] text-glow-cyan bg-glow-cyan/10 border border-glow-cyan/20 px-1.5 py-0.5 rounded">
                  {bp.headcount}
                </span>
                {bp.tags.slice(0, 2).map((t) => (
                  <span
                    key={t}
                    className="text-[10px] text-slate-400 bg-white/[0.03] border border-white/[0.06] px-1.5 py-0.5 rounded"
                  >
                    {t}
                  </span>
                ))}
              </div>
            </button>
          ))}
        </div>
      </div>

      <div className="flex justify-end max-w-5xl mx-auto">
        <GlowButton
          size="lg"
          disabled={idea.trim().length < 10}
          onClick={onNext}
          className="gap-2"
        >
          <Sparkles className="w-4 h-4" />
          Design My AI Team
          <ArrowRight className="w-4 h-4" />
        </GlowButton>
      </div>
    </div>
  );
}

/* ------------------------------------------------------------------ */
/*  Step 2: Designing (loading + result)                               */
/* ------------------------------------------------------------------ */

function DesignStep({
  loading,
  workflow,
  error,
  refining,
  onRetry,
  onRefine,
  onNext,
  onBack,
}: {
  loading: boolean;
  workflow: DesignedWorkflow | null;
  error: string | null;
  refining: boolean;
  onRetry: () => void;
  onRefine: (instruction: string) => Promise<void>;
  onNext: () => void;
  onBack: () => void;
}) {
  const [refineInput, setRefineInput] = useState("");

  async function handleRefine() {
    const instruction = refineInput.trim();
    if (instruction.length < 3 || refining) return;
    await onRefine(instruction);
    setRefineInput("");
  }
  if (loading) {
    return (
      <div className="max-w-2xl mx-auto flex flex-col items-center justify-center py-20 animate-in fade-in duration-300">
        <div className="relative mb-8">
          <div className="w-24 h-24 rounded-3xl bg-gradient-to-br from-glow-cyan/10 to-glow-purple/10 border border-white/[0.08] flex items-center justify-center">
            <Sparkles className="w-10 h-10 text-glow-cyan animate-pulse" />
          </div>
          <div className="absolute -top-2 -right-2 w-8 h-8 rounded-full bg-glow-purple/20 border border-glow-purple/30 flex items-center justify-center">
            <Loader2 className="w-4 h-4 text-glow-purple animate-spin" />
          </div>
        </div>
        <h3 className="text-xl font-bold mb-2">Designing your workflow...</h3>
        <p className="text-slate-400 text-center max-w-md mb-6">
          Nexus is analyzing your idea and designing specialized agents with the right
          roles, tools, and connections.
        </p>
        <div className="flex gap-8 text-xs text-slate-500">
          {["Analyzing intent", "Designing agents", "Mapping connections"].map(
            (label, i) => (
              <div key={label} className="flex items-center gap-2">
                <div
                  className={cn(
                    "w-2 h-2 rounded-full",
                    i === 0
                      ? "bg-emerald-400"
                      : i === 1
                        ? "bg-amber-400 animate-pulse"
                        : "bg-slate-600"
                  )}
                />
                {label}
              </div>
            )
          )}
        </div>
      </div>
    );
  }

  if (error) {
    return (
      <div className="max-w-lg mx-auto flex flex-col items-center justify-center py-20 animate-in fade-in duration-300">
        <div className="w-16 h-16 rounded-2xl bg-red-500/10 border border-red-500/20 flex items-center justify-center mb-4">
          <X className="w-8 h-8 text-red-400" />
        </div>
        <h3 className="text-lg font-bold mb-2">Design failed</h3>
        <p className="text-sm text-slate-400 text-center mb-6">{error}</p>
        <div className="flex gap-3">
          <Button variant="outline" onClick={onBack}>
            <ArrowLeft className="w-4 h-4 mr-2" />
            Edit idea
          </Button>
          <GlowButton onClick={onRetry}>
            <Sparkles className="w-4 h-4 mr-2" />
            Try again
          </GlowButton>
        </div>
      </div>
    );
  }

  if (!workflow) return null;

  const modeConfig =
    EXECUTION_MODE_CONFIG[workflow.execution_mode] ?? EXECUTION_MODE_CONFIG.sequential;
  const ModeIcon = modeConfig.icon;

  return (
    <div className="max-w-4xl mx-auto space-y-6 animate-in fade-in slide-in-from-bottom-4 duration-500">
      {/* Workflow header */}
      <div className="text-center space-y-3">
        <div className="flex items-center justify-center gap-3">
          <Badge
            className={cn("text-xs", COMPLEXITY_COLORS[workflow.estimated_complexity])}
          >
            {workflow.estimated_complexity}
          </Badge>
          <Badge variant="outline" className="text-xs gap-1">
            <ModeIcon className={cn("w-3 h-3", modeConfig.color)} />
            {modeConfig.label}
          </Badge>
        </div>
        <h2 className="text-2xl font-bold">{workflow.title}</h2>
        <p className="text-slate-400 max-w-xl mx-auto">{workflow.description}</p>
        <div className="flex items-center justify-center gap-2 flex-wrap">
          {workflow.tags.map((tag) => (
            <Badge key={tag} variant="secondary" className="text-[10px]">
              {tag}
            </Badge>
          ))}
        </div>
      </div>

      {/* Agent cards */}
      <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
        {workflow.agents.map((agent, i) => (
          <GlowCard
            key={agent.temp_id}
            glow={i === 0 ? "cyan" : i === 1 ? "purple" : "amber"}
            className="p-5"
          >
            <div className="flex items-start gap-3 mb-3">
              <div className="w-12 h-12 rounded-xl bg-white/[0.05] border border-white/[0.08] flex items-center justify-center text-2xl shrink-0">
                {agent.icon}
              </div>
              <div className="min-w-0">
                <h4 className="font-bold text-sm">{agent.name}</h4>
                <p className="text-xs text-slate-400">{agent.role}</p>
              </div>
            </div>

            <p className="text-xs text-slate-400 mb-3 line-clamp-3">
              {agent.description}
            </p>

            {agent.tools.length > 0 && (
              <div className="flex flex-wrap gap-1 mb-3">
                {agent.tools.map((t) => (
                  <Badge key={t} variant="secondary" className="text-[10px] h-5">
                    {t}
                  </Badge>
                ))}
              </div>
            )}

            <div className="flex items-center justify-between text-[10px] text-slate-500">
              <span className="font-mono">{agent.model_suggestion}</span>
              <Badge variant="outline" className="text-[9px] h-4">
                {agent.trigger}
              </Badge>
            </div>
          </GlowCard>
        ))}
      </div>

      {/* Connections */}
      {workflow.connections.length > 0 && (
        <div>
          <h3 className="text-sm font-semibold mb-3 flex items-center gap-2">
            <Network className="w-4 h-4 text-glow-cyan" />
            Agent Connections
          </h3>
          <div className="space-y-2">
            {workflow.connections.map((conn, i) => {
              const from = workflow.agents.find((a) => a.temp_id === conn.from_agent);
              const to = workflow.agents.find((a) => a.temp_id === conn.to_agent);
              return (
                <div
                  key={i}
                  className="flex items-center gap-3 px-4 py-3 rounded-lg border border-white/[0.06] bg-white/[0.02]"
                >
                  <div className="flex items-center gap-2 min-w-0">
                    <span className="text-base">{from?.icon ?? "?"}</span>
                    <span className="text-xs font-medium truncate">
                      {from?.name ?? conn.from_agent}
                    </span>
                  </div>
                  <div className="flex items-center gap-1 text-glow-cyan shrink-0">
                    <div className="w-6 h-px bg-glow-cyan/30" />
                    <ChevronRight className="w-3 h-3" />
                    <div className="w-6 h-px bg-glow-cyan/30" />
                  </div>
                  <div className="flex items-center gap-2 min-w-0">
                    <span className="text-base">{to?.icon ?? "?"}</span>
                    <span className="text-xs font-medium truncate">
                      {to?.name ?? conn.to_agent}
                    </span>
                  </div>
                  <div className="ml-auto text-[10px] text-slate-500 hidden md:block max-w-[200px] truncate">
                    {conn.condition}
                  </div>
                </div>
              );
            })}
          </div>
        </div>
      )}

      {/* Refinement chat — tell the LLM how to change the team */}
      <div className="rounded-xl border border-glow-cyan/10 bg-glow-cyan/[0.02] p-4">
        <div className="flex items-center gap-2 mb-2">
          <Wand2 className="w-4 h-4 text-glow-cyan" />
          <p className="text-xs font-semibold text-white">Refine with chat</p>
          <span className="text-[10px] text-slate-400">
            — talk to the LLM the way you'd talk to a co-founder
          </span>
        </div>
        <div className="flex gap-2">
          <input
            value={refineInput}
            onChange={(e) => setRefineInput(e.target.value)}
            placeholder={
              refining
                ? "Redesigning your team…"
                : "e.g. 'add a QA engineer', 'make the marketer more technical', 'split engineering into frontend and backend'"
            }
            disabled={refining}
            onKeyDown={(e) => {
              if (e.key === "Enter" && !e.shiftKey) {
                e.preventDefault();
                handleRefine();
              }
            }}
            className={cn(
              "flex-1 bg-white/[0.03] border border-white/[0.08] rounded-lg px-3 py-2 text-sm",
              "placeholder:text-slate-500 focus:outline-none focus:border-glow-cyan/30",
              "disabled:opacity-50"
            )}
          />
          <Button
            onClick={handleRefine}
            disabled={refining || refineInput.trim().length < 3}
            className="gap-1.5 bg-gradient-to-r from-glow-cyan to-glow-blue text-white hover:brightness-110"
          >
            {refining ? (
              <Loader2 className="w-3.5 h-3.5 animate-spin" />
            ) : (
              <Sparkles className="w-3.5 h-3.5" />
            )}
            Refine
          </Button>
        </div>
        <div className="mt-2 flex flex-wrap gap-1.5">
          {[
            "add a QA engineer",
            "add a sales team",
            "make them all senior-level",
            "add a CFO for budget oversight",
            "give each agent stricter tool limits",
          ].map((suggestion) => (
            <button
              key={suggestion}
              onClick={() => setRefineInput(suggestion)}
              disabled={refining}
              className="text-[10px] text-slate-400 bg-white/[0.03] hover:bg-white/[0.06] border border-white/[0.06] px-2 py-0.5 rounded transition-colors disabled:opacity-50"
            >
              {suggestion}
            </button>
          ))}
        </div>
      </div>

      {/* Actions */}
      <div className="flex items-center justify-between pt-2">
        <Button variant="outline" onClick={onBack} className="gap-2">
          <ArrowLeft className="w-4 h-4" />
          Edit idea
        </Button>
        <GlowButton size="lg" onClick={onNext} className="gap-2">
          Refine &amp; Configure
          <ArrowRight className="w-4 h-4" />
        </GlowButton>
      </div>
    </div>
  );
}

/* ------------------------------------------------------------------ */
/*  Step 3: Configure                                                  */
/* ------------------------------------------------------------------ */

function ConfigureStep({
  workflow,
  setWorkflow,
  charter,
  setCharter,
  onNext,
  onBack,
}: {
  workflow: DesignedWorkflow;
  setWorkflow: (w: DesignedWorkflow) => void;
  charter: string;
  setCharter: (v: string) => void;
  onNext: () => void;
  onBack: () => void;
}) {
  const [editing, setEditing] = useState<string | null>(null);

  function updateAgent(tempId: string, patch: Partial<DesignedWorkflowAgent>) {
    setWorkflow({
      ...workflow,
      agents: workflow.agents.map((a) =>
        a.temp_id === tempId ? { ...a, ...patch } : a
      ),
    });
  }

  function removeAgent(tempId: string) {
    setWorkflow({
      ...workflow,
      agents: workflow.agents.filter((a) => a.temp_id !== tempId),
      connections: workflow.connections.filter(
        (c) => c.from_agent !== tempId && c.to_agent !== tempId
      ),
    });
  }

  function addAgent() {
    const id = `agent_${Date.now()}`;
    setWorkflow({
      ...workflow,
      agents: [
        ...workflow.agents,
        {
          temp_id: id,
          name: "New Agent",
          role: "Describe this agent's role",
          description: "A new agent",
          system_prompt: "You are a helpful agent.",
          tools: [],
          model_suggestion: "claude-sonnet-4-6",
          trigger: "on_demand",
          icon: "🤖",
        },
      ],
    });
    setEditing(id);
  }

  return (
    <div className="max-w-4xl mx-auto space-y-6 animate-in fade-in slide-in-from-bottom-4 duration-500">
      <div className="text-center space-y-2">
        <h2 className="text-2xl font-bold">Refine Your AI Team</h2>
        <p className="text-slate-400">
          Fine-tune each employee, set a company charter, then hire them all.
        </p>
      </div>

      {/* Company charter — shared context every agent gets */}
      <Card className="border-glow-cyan/15 bg-glow-cyan/[0.02]">
        <CardContent className="p-4 space-y-2">
          <div className="flex items-center gap-2">
            <Rocket className="w-4 h-4 text-glow-cyan" />
            <p className="text-sm font-semibold">Company Charter</p>
            <Badge variant="outline" className="text-[9px] h-4 px-1.5">
              shared with every agent
            </Badge>
          </div>
          <p className="text-[11px] text-slate-400">
            A short mission statement prepended to every agent's system prompt
            so the whole team pulls in one direction.
          </p>
          <textarea
            value={charter}
            onChange={(e) => setCharter(e.target.value)}
            placeholder="e.g. We are an early-stage B2B SaaS. Our mission is to help small teams ship production-grade infrastructure in minutes. Optimize for speed, simplicity, and delight. Never ship without tests."
            rows={3}
            className="w-full bg-white/[0.03] border border-white/[0.08] rounded-lg p-3 text-sm text-white placeholder:text-slate-500 resize-none focus:outline-none focus:border-glow-cyan/30 leading-relaxed"
          />
          <p className="text-[10px] text-slate-500">
            Optional — leave blank to skip.
          </p>
        </CardContent>
      </Card>

      <div className="space-y-4">
        {workflow.agents.map((agent) => {
          const isEditing = editing === agent.temp_id;

          return (
            <Card
              key={agent.temp_id}
              className={cn(
                "overflow-hidden transition-all duration-300",
                isEditing && "border-glow-cyan/20 shadow-[0_0_20px_rgba(0,200,255,0.06)]"
              )}
            >
              {/* Header */}
              <div
                className="flex items-center gap-4 p-4 cursor-pointer hover:bg-white/[0.02] transition-colors"
                onClick={() => setEditing(isEditing ? null : agent.temp_id)}
              >
                <div className="w-12 h-12 rounded-xl bg-white/[0.05] border border-white/[0.08] flex items-center justify-center text-2xl shrink-0">
                  {agent.icon}
                </div>
                <div className="flex-1 min-w-0">
                  <div className="flex items-center gap-2 mb-0.5">
                    <span className="font-bold text-sm">{agent.name}</span>
                    <Badge variant="outline" className="text-[10px] font-mono h-4">
                      {agent.model_suggestion}
                    </Badge>
                  </div>
                  <p className="text-xs text-slate-400 truncate">{agent.role}</p>
                </div>
                <div className="flex items-center gap-2">
                  <Button
                    variant="ghost"
                    size="icon"
                    className="h-7 w-7 text-slate-400 hover:text-red-400"
                    onClick={(e) => {
                      e.stopPropagation();
                      removeAgent(agent.temp_id);
                    }}
                  >
                    <Trash2 className="w-3.5 h-3.5" />
                  </Button>
                  <Pencil
                    className={cn(
                      "w-4 h-4 transition-colors",
                      isEditing ? "text-glow-cyan" : "text-slate-500"
                    )}
                  />
                </div>
              </div>

              {/* Edit panel */}
              {isEditing && (
                <>
                  <Separator />
                  <CardContent className="pt-4 pb-5 space-y-4">
                    <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
                      <div>
                        <label className="text-[11px] font-medium text-slate-400 uppercase tracking-wider mb-1.5 block">
                          Name
                        </label>
                        <Input
                          value={agent.name}
                          onChange={(e) =>
                            updateAgent(agent.temp_id, { name: e.target.value })
                          }
                          className="bg-white/[0.03] border-white/[0.08]"
                        />
                      </div>
                      <div>
                        <label className="text-[11px] font-medium text-slate-400 uppercase tracking-wider mb-1.5 block">
                          Role
                        </label>
                        <Input
                          value={agent.role}
                          onChange={(e) =>
                            updateAgent(agent.temp_id, { role: e.target.value })
                          }
                          className="bg-white/[0.03] border-white/[0.08]"
                        />
                      </div>
                    </div>

                    <div>
                      <label className="text-[11px] font-medium text-slate-400 uppercase tracking-wider mb-1.5 block">
                        System Prompt
                      </label>
                      <textarea
                        value={agent.system_prompt}
                        onChange={(e) =>
                          updateAgent(agent.temp_id, {
                            system_prompt: e.target.value,
                          })
                        }
                        rows={4}
                        className="w-full bg-white/[0.03] border border-white/[0.08] rounded-lg p-3 text-sm text-white placeholder:text-slate-500 resize-none focus:outline-none focus:border-glow-cyan/30"
                      />
                    </div>

                    <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
                      <div>
                        <label className="text-[11px] font-medium text-slate-400 uppercase tracking-wider mb-1.5 block">
                          Model
                        </label>
                        <div className="flex gap-2">
                          {["claude-sonnet-4-6", "gpt-4.1", "gpt-4.1-mini"].map(
                            (m) => (
                              <button
                                key={m}
                                onClick={() =>
                                  updateAgent(agent.temp_id, {
                                    model_suggestion: m,
                                  })
                                }
                                className={cn(
                                  "px-3 py-1.5 rounded-lg text-xs font-mono border transition-all",
                                  agent.model_suggestion === m
                                    ? "border-glow-cyan/30 bg-glow-cyan/10 text-white"
                                    : "border-white/[0.08] bg-white/[0.02] text-slate-400 hover:bg-white/[0.04]"
                                )}
                              >
                                {m}
                              </button>
                            )
                          )}
                        </div>
                      </div>
                      <div>
                        <label className="text-[11px] font-medium text-slate-400 uppercase tracking-wider mb-1.5 block">
                          Trigger
                        </label>
                        <div className="flex gap-2 flex-wrap">
                          {["on_demand", "scheduled", "event_driven", "always_on"].map(
                            (t) => (
                              <button
                                key={t}
                                onClick={() =>
                                  updateAgent(agent.temp_id, { trigger: t })
                                }
                                className={cn(
                                  "px-3 py-1.5 rounded-lg text-xs border transition-all",
                                  agent.trigger === t
                                    ? "border-glow-cyan/30 bg-glow-cyan/10 text-white"
                                    : "border-white/[0.08] bg-white/[0.02] text-slate-400 hover:bg-white/[0.04]"
                                )}
                              >
                                {t.replace("_", " ")}
                              </button>
                            )
                          )}
                        </div>
                      </div>
                    </div>

                    <div>
                      <label className="text-[11px] font-medium text-slate-400 uppercase tracking-wider mb-1.5 block">
                        Tools (comma-separated)
                      </label>
                      <Input
                        value={agent.tools.join(", ")}
                        onChange={(e) =>
                          updateAgent(agent.temp_id, {
                            tools: e.target.value
                              .split(",")
                              .map((t) => t.trim())
                              .filter(Boolean),
                          })
                        }
                        placeholder="search_web, send_email, query_database"
                        className="bg-white/[0.03] border-white/[0.08]"
                      />
                    </div>
                  </CardContent>
                </>
              )}
            </Card>
          );
        })}

        {/* Add agent */}
        <button
          onClick={addAgent}
          className="w-full p-4 rounded-xl border border-dashed border-white/[0.08] hover:border-white/[0.15] bg-white/[0.01] hover:bg-white/[0.03] transition-all flex items-center justify-center gap-2 text-sm text-slate-400 hover:text-white"
        >
          <Plus className="w-4 h-4" />
          Add Agent
        </button>
      </div>

      <div className="flex items-center justify-between pt-2">
        <Button variant="outline" onClick={onBack} className="gap-2">
          <ArrowLeft className="w-4 h-4" />
          Back to Design
        </Button>
        <GlowButton
          size="lg"
          onClick={onNext}
          disabled={workflow.agents.length === 0}
          className="gap-2"
        >
          <Rocket className="w-4 h-4" />
          Review &amp; Hire {workflow.agents.length}{" "}
          {workflow.agents.length !== 1 ? "Employees" : "Employee"}
          <ArrowRight className="w-4 h-4" />
        </GlowButton>
      </div>
    </div>
  );
}

/* ------------------------------------------------------------------ */
/*  Step 4: Deploy                                                     */
/* ------------------------------------------------------------------ */

function DeployStep({
  deploying,
  deployed,
  agentCount,
  error,
  onDeploy,
  onGoToAgents,
  onBack,
}: {
  deploying: boolean;
  deployed: boolean;
  agentCount: number;
  error: string | null;
  onDeploy: () => void;
  onGoToAgents: () => void;
  onBack: () => void;
}) {
  if (deployed) {
    return (
      <div className="max-w-lg mx-auto flex flex-col items-center justify-center py-20 animate-in fade-in zoom-in-95 duration-500">
        <div className="relative mb-6">
          <div className="w-20 h-20 rounded-2xl bg-gradient-to-br from-emerald-500/20 to-emerald-400/10 border border-emerald-500/30 flex items-center justify-center">
            <Check className="w-10 h-10 text-emerald-400" />
          </div>
          <div className="absolute -top-1 -right-1 w-8 h-8 rounded-full bg-emerald-500/20 border border-emerald-500/30 flex items-center justify-center animate-bounce">
            <Zap className="w-4 h-4 text-emerald-400" />
          </div>
        </div>
        <h2 className="text-2xl font-bold mb-2">Your AI Team is Hired!</h2>
        <p className="text-slate-400 text-center mb-8">
          {agentCount} AI employee{agentCount !== 1 ? "s are" : " is"} onboarded
          and ready to start working.
        </p>
        <div className="flex gap-3">
          <Button variant="outline" onClick={onGoToAgents} className="gap-2">
            <Bot className="w-4 h-4" />
            Meet the Team
          </Button>
          <GlowButton onClick={onGoToAgents} className="gap-2">
            <Play className="w-4 h-4" />
            Start Working
          </GlowButton>
        </div>
      </div>
    );
  }

  return (
    <div className="max-w-lg mx-auto flex flex-col items-center justify-center py-16 animate-in fade-in slide-in-from-bottom-4 duration-500">
      <div className="w-20 h-20 rounded-2xl bg-gradient-to-br from-glow-cyan/20 to-glow-purple/20 border border-white/[0.1] flex items-center justify-center mb-6">
        <Rocket className="w-10 h-10 text-glow-cyan" />
      </div>
      <h2 className="text-2xl font-bold mb-2">Ready to Hire</h2>
      <p className="text-slate-400 text-center mb-2">
        This will onboard {agentCount} AI employee
        {agentCount !== 1 ? "s" : ""} into your project with their roles,
        tools, and shared company charter.
      </p>
      <p className="text-xs text-slate-500 mb-8">
        You can promote, re-brief, or dismiss any employee later.
      </p>

      {error && (
        <div className="w-full mb-6 p-4 rounded-lg border border-red-500/20 bg-red-500/10 text-sm text-red-400">
          {error}
        </div>
      )}

      <div className="flex gap-3">
        <Button variant="outline" onClick={onBack} className="gap-2">
          <ArrowLeft className="w-4 h-4" />
          Back
        </Button>
        <GlowButton
          size="lg"
          onClick={onDeploy}
          disabled={deploying}
          className="gap-2 min-w-[200px]"
        >
          {deploying ? (
            <>
              <Loader2 className="w-4 h-4 animate-spin" />
              Hiring…
            </>
          ) : (
            <>
              <Rocket className="w-4 h-4" />
              Hire My AI Team
            </>
          )}
        </GlowButton>
      </div>
    </div>
  );
}

/* ------------------------------------------------------------------ */
/*  Main Page                                                          */
/* ------------------------------------------------------------------ */

export default function CreateAgentWorkflowPage() {
  const params = useParams();
  const router = useRouter();
  const projectId = params.projectId as string;

  const [step, setStep] = useState<StepId>("idea");
  const [completedSteps, setCompletedSteps] = useState<Set<StepId>>(new Set());
  const [idea, setIdea] = useState("");
  const [workflow, setWorkflow] = useState<DesignedWorkflow | null>(null);
  const [designing, setDesigning] = useState(false);
  const [designError, setDesignError] = useState<string | null>(null);
  const [refining, setRefining] = useState(false);
  const [charter, setCharter] = useState("");
  const [deploying, setDeploying] = useState(false);
  const [deployed, setDeployed] = useState(false);
  const [deployError, setDeployError] = useState<string | null>(null);
  const [agentCount, setAgentCount] = useState(0);

  const markComplete = useCallback(
    (s: StepId) => setCompletedSteps((prev) => new Set([...prev, s])),
    []
  );

  async function handleDesign() {
    setStep("design");
    setDesigning(true);
    setDesignError(null);
    setWorkflow(null);

    try {
      const result = await api.designWorkflow(projectId, idea);
      setWorkflow(result.workflow);
      markComplete("idea");
    } catch (e) {
      setDesignError(e instanceof Error ? e.message : "Failed to design workflow");
    } finally {
      setDesigning(false);
    }
  }

  async function handleRefine(instruction: string) {
    if (!workflow) return;
    setRefining(true);
    setDesignError(null);
    try {
      const roster = workflow.agents
        .map((a) => `- ${a.name} (${a.role})`)
        .join("\n");
      const augmented = `Current team for "${workflow.title}":
${roster}

Original brief: ${idea}

Please redesign the team based on this instruction: ${instruction}

Keep roles that still make sense, modify where the instruction asks, and add new roles as needed. Preserve the overall mission.`;
      const result = await api.designWorkflow(projectId, augmented);
      setWorkflow(result.workflow);
      setIdea(augmented);
    } catch (e) {
      setDesignError(e instanceof Error ? e.message : "Failed to refine team");
    } finally {
      setRefining(false);
    }
  }

  async function handleDeploy() {
    if (!workflow) return;
    setDeploying(true);
    setDeployError(null);

    try {
      const trimmedCharter = charter.trim();
      const workflowToShip = trimmedCharter
        ? {
            ...workflow,
            agents: workflow.agents.map((a) => ({
              ...a,
              system_prompt: `## Company Charter\n${trimmedCharter}\n\n## Your Role\n${a.system_prompt}`,
            })),
          }
        : workflow;

      const result = await api.materializeWorkflow(projectId, workflowToShip);
      if (result.success) {
        setAgentCount(result.agents_created);
        setDeployed(true);
        markComplete("deploy");
      } else {
        setDeployError("No agents could be hired. Please try again.");
      }
    } catch (e) {
      setDeployError(e instanceof Error ? e.message : "Hiring failed");
    } finally {
      setDeploying(false);
    }
  }

  return (
    <div className="h-full overflow-y-auto scrollbar-thin">
      {/* Top bar */}
      <div className="sticky top-0 z-10 border-b border-white/[0.06] bg-nexus-deep/80 backdrop-blur-xl px-6 py-3">
        <div className="flex items-center justify-between max-w-4xl mx-auto">
          <button
            onClick={() => router.push(`/${projectId}/agents`)}
            className="flex items-center gap-2 text-sm text-slate-400 hover:text-white transition-colors"
          >
            <ArrowLeft className="w-4 h-4" />
            Agents
          </button>
          <StepIndicator currentStep={step} completedSteps={completedSteps} />
          <div className="w-16" />
        </div>
      </div>

      {/* Content */}
      <div className="p-6 md:p-10">
        {step === "idea" && (
          <IdeaStep idea={idea} setIdea={setIdea} onNext={handleDesign} />
        )}

        {step === "design" && (
          <DesignStep
            loading={designing}
            workflow={workflow}
            error={designError}
            refining={refining}
            onRetry={handleDesign}
            onRefine={handleRefine}
            onNext={() => {
              markComplete("design");
              setStep("configure");
            }}
            onBack={() => setStep("idea")}
          />
        )}

        {step === "configure" && workflow && (
          <ConfigureStep
            workflow={workflow}
            setWorkflow={setWorkflow}
            charter={charter}
            setCharter={setCharter}
            onNext={() => {
              markComplete("configure");
              setStep("deploy");
            }}
            onBack={() => setStep("design")}
          />
        )}

        {step === "deploy" && (
          <DeployStep
            deploying={deploying}
            deployed={deployed}
            agentCount={agentCount || (workflow?.agents.length ?? 0)}
            error={deployError}
            onDeploy={handleDeploy}
            onGoToAgents={() => router.push(`/${projectId}/agents`)}
            onBack={() => setStep("configure")}
          />
        )}
      </div>
    </div>
  );
}
