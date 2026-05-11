"use client";

import { useState, useEffect, useRef } from "react";
import {
  Brain, Sparkles, Globe, Shield, Zap, Layers, Eye,
  CheckCircle2, TrendingUp,
  Code2, Palette, BarChart2, FileCode, Clock,
  ChevronDown, ChevronRight, Lightbulb,
  Monitor, ExternalLink,
} from "lucide-react";
import { cn } from "@/lib/utils";
import type { OneShotEvent } from "@/lib/api";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export interface ThinkingStep {
  id: string;
  type: "thinking" | "decision" | "phase" | "file" | "score" | "complete" | "error" | "insight";
  icon: string;
  message: string;
  detail?: string;
  progress?: number;
  timestamp: number;
  // Decision-specific — now interactive
  decision?: string;
  reason?: string;
  confidence?: number;
  alternatives?: string[];
  impact?: string;
  area?: string;
  chosenValue?: string;
  // Score-specific
  score?: number;
  scoreBefore?: number;
  scoreAfter?: number;
  // Complete-specific
  projectId?: string;
  projectName?: string;
  tasteScore?: number;
  filesCount?: number;
  durationMs?: number;
  appPort?: number;
  // Error-specific
  fatal?: boolean;
}

/** Post-generation improvement suggestion from the system */
export interface Improvement {
  id: string;
  title: string;
  description: string;
  impact: string;
  priority: "high" | "medium" | "low";
  category: "ux" | "performance" | "accessibility" | "security" | "feature";
}

// ---------------------------------------------------------------------------
// Convert SSE events to human-readable thinking steps
// ---------------------------------------------------------------------------

const ICON_MAP: Record<string, typeof Brain> = {
  brain: Brain, sparkle: Sparkles, globe: Globe, shield: Shield,
  lightning: Zap, layers: Layers, eye: Eye, check: CheckCircle2,
  compress: BarChart2, code: Code2, palette: Palette,
};

function getIcon(name: string) {
  return ICON_MAP[name] || Brain;
}

// Impact phrases for different decision areas
const IMPACT_MAP: Record<string, string> = {
  frontend: "Affects page load speed, SEO, and developer experience",
  database: "Affects data reliability, query performance, and scaling cost",
  auth: "Affects security posture, onboarding friction, and compliance",
  hosting: "Affects deployment speed, cost, and geographic availability",
  ui_library: "Affects design consistency, accessibility, and development speed",
};

// Tradeoff phrases for alternatives
const TRADEOFF_MAP: Record<string, Record<string, string>> = {
  frontend: {
    NextjsAppRouter: "Best for SEO + server rendering, heavier initial setup",
    StaticHtml: "Fastest load, but no interactivity or server logic",
    ReactSpa: "Rich interactivity, but poor SEO without extra work",
  },
  database: {
    Postgres: "Production-grade, but requires hosting infrastructure",
    Sqlite: "Zero-config, but limited concurrent writes and no cloud scaling",
    None: "Simplest, but data is lost on restart",
  },
  auth: {
    NextAuth: "Full OAuth flows, but complex to configure correctly",
    SimpleCredentials: "Quick to build, but limited security features",
    ClerkAuth: "Best UX, but adds external dependency and cost",
    None: "No barrier to entry, but no user identity",
  },
};

export function eventToStep(event: OneShotEvent, index: number): ThinkingStep | null {
  const id = `step-${index}-${Date.now()}`;
  const now = Date.now();

  switch (event.type) {
    case "thinking":
      return {
        id, type: "thinking", icon: event.icon || "brain",
        message: event.message || "Thinking...",
        detail: event.detail || undefined,
        progress: event.progress,
        timestamp: now,
      };

    case "intent_analyzed":
      return {
        id, type: "insight", icon: "eye",
        message: `Understood: ${event.app_type?.replace(/([A-Z])/g, " $1").trim()} (${event.complexity})`,
        detail: [
          event.domain ? `Domain: ${event.domain}` : null,
          event.needs_auth ? "Requires authentication" : null,
          event.needs_database ? "Requires database" : null,
        ].filter(Boolean).join(" \u00B7 "),
        impact: "This classification drives every decision below",
        timestamp: now,
      };

    case "decisions_made": {
      const overrides = event.learning_overrides || [];
      return {
        id, type: "decision", icon: "sparkle",
        message: "Architecture chosen",
        detail: [
          event.frontend ? `Frontend: ${event.frontend.replace(/([A-Z])/g, " $1")}` : null,
          event.database ? `Database: ${event.database}` : null,
          event.auth && event.auth !== "None" ? `Auth: ${event.auth}` : null,
        ].filter(Boolean).join(" \u00B7 "),
        decision: "Architecture",
        reason: overrides.length
          ? `Optimized from ${overrides.length} past project(s)`
          : "Selected based on your requirements",
        confidence: overrides.length ? 0.92 : 0.85,
        alternatives: [],
        impact: "Defines the entire tech stack, deployment, and maintenance cost",
        area: "architecture",
        chosenValue: event.frontend || "",
        timestamp: now,
      };
    }

    case "product_brief_ready":
      return {
        id, type: "insight", icon: "palette",
        message: event.hero_headline || "Product designed",
        detail: `${event.personas || 0} personas \u00B7 ${event.features || 0} features`,
        impact: "Real product copy and user flows \u2014 not placeholder text",
        timestamp: now,
      };

    case "explanation": {
      const area = event.decision?.toLowerCase().replace(/\s+/g, "_") || "";
      const alternatives = event.alternatives || [];
      return {
        id, type: "decision", icon: "brain",
        message: event.decision || "Decision made",
        detail: event.reason,
        decision: event.decision,
        reason: event.reason,
        confidence: event.confidence,
        alternatives,
        impact: IMPACT_MAP[area] || "Affects the quality and maintainability of your app",
        area,
        timestamp: now,
      };
    }

    case "files_generated":
      return {
        id, type: "file", icon: "code",
        message: `${event.count || event.files_count || 0} files generated`,
        detail: "Complete application with routes, components, and API endpoints",
        impact: "Production-ready code \u2014 not scaffolding",
        timestamp: now,
      };

    case "taste_scored":
      return {
        id, type: "score", icon: "sparkle",
        message: event.redesign_triggered
          ? `Quality: ${event.overall}/100 \u2014 improving automatically`
          : `Quality: ${event.overall}/100`,
        score: event.overall,
        impact: event.redesign_triggered
          ? "Below threshold \u2014 auto-fixing UX issues now"
          : "Meets quality standards across 6 UX dimensions",
        timestamp: now,
      };

    case "redesign_complete":
      return {
        id, type: "score", icon: "sparkle",
        message: `Quality improved: ${event.score_before} \u2192 ${event.score_after}`,
        detail: `${event.mutations_applied} improvements applied`,
        score: event.score_after,
        scoreBefore: event.score_before,
        scoreAfter: event.score_after,
        impact: `+${(event.score_after || 0) - (event.score_before || 0)} points from automated UX fixes`,
        timestamp: now,
      };

    case "complete":
      return {
        id, type: "complete", icon: "check",
        message: "Your app is ready",
        projectId: event.project_id,
        projectName: event.project_name,
        tasteScore: event.taste_score,
        filesCount: event.files_count,
        durationMs: event.duration_ms,
        timestamp: now,
      };

    case "error":
      return {
        id, type: "error", icon: "shield",
        message: event.message || "Something went wrong",
        detail: event.phase ? `During: ${event.phase}` : undefined,
        fatal: event.fatal,
        timestamp: now,
      };

    case "phase": {
      if (event.status === "started" && event.phase && event.detail) {
        const phaseIcons: Record<string, string> = {
          learning: "brain",
          intent: "eye",
          decisions: "sparkle",
          project: "code",
          codegen: "code",
          taste: "sparkle",
          redesign: "palette",
        };
        return {
          id, type: "phase", icon: phaseIcons[event.phase] || "brain",
          message: event.detail,
          timestamp: now,
        };
      }
      return null;
    }

    case "progress": {
      return null;
    }

    case "skeleton": {
      if (event.path) {
        return {
          id, type: "file", icon: "code",
          message: `Scaffolding ${event.path}`,
          detail: event.skeleton_type ? `${event.skeleton_type} skeleton` : undefined,
          timestamp: now,
        };
      }
      return null;
    }

    case "file_written": {
      if (event.path) {
        return {
          id, type: "file", icon: "code",
          message: `Wrote ${event.path}`,
          detail: event.lines ? `${event.lines} lines` : undefined,
          timestamp: now,
        };
      }
      return null;
    }

    case "heartbeat": {
      if (event.message) {
        return {
          id, type: "thinking", icon: "brain",
          message: event.message,
          timestamp: now,
        };
      }
      return null;
    }

    case "estimate": {
      if (event.total_estimated_ms) {
        const secs = Math.round(event.total_estimated_ms / 1000);
        return {
          id, type: "thinking", icon: "lightning",
          message: `Estimated time: ~${secs}s`,
          detail: event.confidence != null
            ? `${Math.round(event.confidence * 100)}% confidence based on past builds`
            : undefined,
          timestamp: now,
        };
      }
      return null;
    }

    case "project_created": {
      return {
        id, type: "phase", icon: "check",
        message: `Project "${event.project_name}" created`,
        timestamp: now,
      };
    }

    default:
      return null;
  }
}

// ---------------------------------------------------------------------------
// Interactive decision step
// ---------------------------------------------------------------------------

function DecisionBubble({
  step,
  isLatest,
  onOverride,
}: {
  step: ThinkingStep;
  isLatest: boolean;
  onOverride?: (area: string, newValue: string) => void;
}) {
  const [expanded, setExpanded] = useState(false);
  const Icon = getIcon(step.icon);
  const hasAlternatives = step.alternatives && step.alternatives.length > 0;

  return (
    <div className="step-appear rounded-xl border bg-violet-400/10 border-violet-400/20">
      {/* Main row */}
      <div className="flex items-start gap-3 px-4 py-3">
        <div className={cn("mt-0.5 flex-shrink-0 text-violet-400", isLatest && "thinking-pulse")}>
          <Icon className="w-4 h-4" />
        </div>
        <div className="flex-1 min-w-0">
          <p className="text-sm font-medium text-slate-200">{step.message}</p>
          {step.detail && (
            <p className="text-xs text-slate-400 mt-0.5">{step.detail}</p>
          )}
          {/* Impact line */}
          {step.impact && (
            <p className="text-[11px] text-violet-300/60 mt-1 flex items-center gap-1">
              <Lightbulb className="w-3 h-3 inline flex-shrink-0" />
              {step.impact}
            </p>
          )}
          {/* Confidence bar */}
          {step.confidence != null && (
            <div className="mt-2 flex items-center gap-2">
              <div className="flex-1 h-1 bg-white/[0.06] rounded-full overflow-hidden">
                <div
                  className="h-full bg-violet-400/60 rounded-full transition-all duration-500"
                  style={{ width: `${step.confidence * 100}%` }}
                />
              </div>
              <span className="text-[10px] text-violet-400/70 whitespace-nowrap">
                {Math.round(step.confidence * 100)}% confident
              </span>
            </div>
          )}
        </div>
        {/* Expand toggle for alternatives */}
        {hasAlternatives && (
          <button
            onClick={() => setExpanded(!expanded)}
            className="mt-0.5 p-1 rounded hover:bg-white/[0.06] text-slate-400 transition-colors"
            title="Show alternatives"
          >
            {expanded ? <ChevronDown className="w-3.5 h-3.5" /> : <ChevronRight className="w-3.5 h-3.5" />}
          </button>
        )}
      </div>

      {/* Expanded: alternatives with tradeoffs */}
      {expanded && hasAlternatives && (
        <div className="px-4 pb-3 pt-0 border-t border-violet-400/10">
          <p className="text-[10px] text-slate-400/60 uppercase tracking-wider mb-2 mt-2">
            Alternatives considered
          </p>
          <div className="space-y-1.5">
            {step.alternatives!.map((alt, i) => {
              const tradeoffs = step.area ? TRADEOFF_MAP[step.area] : undefined;
              const tradeoff = tradeoffs?.[alt.split(":")[0]?.trim()] || "";
              return (
                <div
                  key={i}
                  className="flex items-center gap-2 px-3 py-2 rounded-lg bg-white/[0.03] border border-white/[0.05] group"
                >
                  <div className="flex-1 min-w-0">
                    <p className="text-xs text-slate-400">{alt}</p>
                    {tradeoff && (
                      <p className="text-[10px] text-slate-400/50 mt-0.5">{tradeoff}</p>
                    )}
                  </div>
                  {onOverride && step.area && (
                    <button
                      onClick={() => onOverride(step.area!, alt.split(":")[0]?.trim() || alt)}
                      className="opacity-0 group-hover:opacity-100 text-[10px] text-violet-400 hover:text-violet-300 whitespace-nowrap transition-opacity"
                    >
                      Use this
                    </button>
                  )}
                </div>
              );
            })}
          </div>
        </div>
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Standard step bubble (non-interactive)
// ---------------------------------------------------------------------------

function StepBubble({
  step,
  isLatest,
  onFixWithAi,
}: {
  step: ThinkingStep;
  isLatest: boolean;
  onFixWithAi?: (step: ThinkingStep) => void | Promise<void>;
}) {
  const [fixing, setFixing] = useState(false);
  const [fixed, setFixed] = useState(false);
  const Icon = getIcon(step.icon);

  const colorClass = step.type === "error" ? "text-red-400"
    : step.type === "complete" ? "text-emerald-400"
    : step.type === "score" ? "text-amber-400"
    : step.type === "insight" ? "text-sky-400"
    : step.type === "phase" ? "text-cyan-400"
    : step.type === "file" ? "text-emerald-400"
    : "text-orange-400";

  const bgClass = step.type === "error" ? "bg-red-400/10 border-red-400/20"
    : step.type === "complete" ? "bg-emerald-400/10 border-emerald-400/20"
    : step.type === "score" ? "bg-amber-400/10 border-amber-400/20"
    : step.type === "insight" ? "bg-sky-400/10 border-sky-400/20"
    : step.type === "phase" ? "bg-cyan-400/10 border-cyan-400/20"
    : step.type === "file" ? "bg-emerald-400/5 border-emerald-400/10"
    : "bg-white/[0.04] border-white/[0.08]";

  return (
    <div className={cn("step-appear flex items-start gap-3 px-4 py-3 rounded-xl border", bgClass)}>
      <div className={cn("mt-0.5 flex-shrink-0", colorClass, isLatest && "thinking-pulse")}>
        <Icon className="w-4 h-4" />
      </div>
      <div className="flex-1 min-w-0">
        <p className={cn("text-sm font-medium", step.type === "complete" ? "text-emerald-300" : "text-slate-200")}>
          {step.message}
        </p>
        {step.detail && (
          <p className="text-xs text-slate-400 mt-0.5">{step.detail}</p>
        )}
        {/* Impact line */}
        {step.impact && step.type !== "complete" && (
          <p className={cn(
            "text-[11px] mt-1 flex items-center gap-1",
            step.type === "score" ? "text-amber-300/60" :
            step.type === "insight" ? "text-sky-300/60" :
            step.type === "error" ? "text-red-300/60" :
            "text-slate-400/50"
          )}>
            <Lightbulb className="w-3 h-3 inline flex-shrink-0" />
            {step.impact}
          </p>
        )}
        {/* Fix-with-AI button for error steps */}
        {step.type === "error" && onFixWithAi && !fixed && (
          <button
            type="button"
            onClick={async () => {
              setFixing(true);
              try {
                await onFixWithAi(step);
                setFixed(true);
              } finally {
                setFixing(false);
              }
            }}
            disabled={fixing}
            className="mt-2 inline-flex items-center gap-1.5 px-2.5 py-1 rounded-md text-[11px] font-medium bg-red-500/15 hover:bg-red-500/25 text-red-200 border border-red-400/25 transition-colors disabled:opacity-60"
          >
            {fixing ? (
              <>
                <svg className="w-3 h-3 animate-spin" viewBox="0 0 24 24" fill="none">
                  <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4" />
                  <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8v4a4 4 0 00-4 4H4z" />
                </svg>
                Fixing…
              </>
            ) : (
              <>
                <Sparkles className="w-3 h-3" />
                Fix with AI
              </>
            )}
          </button>
        )}
        {step.type === "error" && fixed && (
          <p className="mt-2 text-[11px] text-emerald-300 flex items-center gap-1">
            <CheckCircle2 className="w-3 h-3" />
            Fix applied — re-run generation to verify.
          </p>
        )}
        {/* Score bar */}
        {step.type === "score" && step.score != null && (
          <div className="mt-2 flex items-center gap-2">
            <div className="flex-1 h-1.5 bg-white/[0.06] rounded-full overflow-hidden">
              <div
                className={cn(
                  "h-full rounded-full transition-all duration-700",
                  step.score >= 85 ? "bg-emerald-400 progress-glow" :
                  step.score >= 70 ? "bg-amber-400" : "bg-red-400"
                )}
                style={{ width: `${step.score}%` }}
              />
            </div>
            <span className={cn(
              "text-xs font-bold score-pop",
              step.score >= 85 ? "text-emerald-400" :
              step.score >= 70 ? "text-amber-400" : "text-red-400"
            )}>
              {step.score}
            </span>
          </div>
        )}
        {/* Complete summary */}
        {step.type === "complete" && (
          <div className="mt-3 flex items-center gap-4 text-xs text-slate-400">
            {step.filesCount != null && (
              <span className="flex items-center gap-1">
                <FileCode className="w-3 h-3" /> {step.filesCount} files
              </span>
            )}
            {step.tasteScore != null && (
              <span className="flex items-center gap-1">
                <TrendingUp className="w-3 h-3" /> {step.tasteScore}/100 quality
              </span>
            )}
            {step.durationMs != null && (
              <span className="flex items-center gap-1">
                <Clock className="w-3 h-3" /> {(step.durationMs / 1000).toFixed(1)}s
              </span>
            )}
          </div>
        )}
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Thinking dots
// ---------------------------------------------------------------------------

function ThinkingDots() {
  return (
    <div className="flex items-center gap-1.5 px-4 py-2">
      <div className="w-1.5 h-1.5 rounded-full bg-orange-400/60 thinking-dot" />
      <div className="w-1.5 h-1.5 rounded-full bg-orange-400/60 thinking-dot" />
      <div className="w-1.5 h-1.5 rounded-full bg-orange-400/60 thinking-dot" />
    </div>
  );
}

// ---------------------------------------------------------------------------
// Progress ring
// ---------------------------------------------------------------------------

function ProgressRing({ progress }: { progress: number }) {
  const radius = 32;
  const circumference = 2 * Math.PI * radius;
  const offset = circumference - (progress / 100) * circumference;

  return (
    <div className="relative w-20 h-20 flex items-center justify-center">
      <svg className="w-20 h-20 -rotate-90" viewBox="0 0 80 80">
        <circle
          cx="40" cy="40" r={radius}
          fill="none" stroke="rgba(255,255,255,0.06)" strokeWidth="3"
        />
        <circle
          cx="40" cy="40" r={radius}
          fill="none" stroke="url(#progressGrad)" strokeWidth="3"
          strokeLinecap="round"
          strokeDasharray={circumference}
          strokeDashoffset={offset}
          className="transition-all duration-500"
        />
        <defs>
          <linearGradient id="progressGrad" x1="0%" y1="0%" x2="100%" y2="0%">
            <stop offset="0%" stopColor="#f97316" />
            <stop offset="100%" stopColor="#a855f7" />
          </linearGradient>
        </defs>
      </svg>
      <div className="absolute inset-0 flex items-center justify-center">
        <span className="text-lg font-bold neon-text">{progress}%</span>
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Proactive improvements panel
// ---------------------------------------------------------------------------

function ImprovementsSuggestion({
  tasteScore,
  onImprove,
}: {
  tasteScore?: number;
  onImprove: (id: string) => void | Promise<void>;
}) {
  const [runningId, setRunningId] = useState<string | null>(null);
  const improvements: Improvement[] = [];

  if (tasteScore != null && tasteScore < 90) {
    improvements.push({
      id: "ux-polish",
      title: "Polish UX details",
      description: "Add loading states, empty states, and micro-interactions",
      impact: `+${Math.min(10, 90 - tasteScore)} quality points estimated`,
      priority: "high",
      category: "ux",
    });
  }

  improvements.push(
    {
      id: "a11y",
      title: "Improve accessibility",
      description: "Add ARIA labels, keyboard navigation, and color contrast fixes",
      impact: "Makes your app usable by 15% more users",
      priority: "medium",
      category: "accessibility",
    },
    {
      id: "perf",
      title: "Optimize performance",
      description: "Add image optimization, code splitting, and caching headers",
      impact: "Faster page loads improve conversion by ~7% per 100ms saved",
      priority: "medium",
      category: "performance",
    },
    {
      id: "seo",
      title: "Add SEO metadata",
      description: "Open Graph tags, structured data, and sitemap generation",
      impact: "Required for organic search visibility",
      priority: "low",
      category: "feature",
    },
  );

  const priorityColor = {
    high: "text-amber-400 bg-amber-400/10 border-amber-400/20",
    medium: "text-sky-400 bg-sky-400/10 border-sky-400/20",
    low: "text-slate-400 bg-white/[0.04] border-white/[0.08]",
  };

  return (
    <div className="step-appear mt-4">
      <div className="flex items-center gap-2 mb-3">
        <Lightbulb className="w-4 h-4 text-amber-400" />
        <p className="text-sm font-medium text-slate-200">What I&apos;d improve next</p>
      </div>
      <div className="space-y-2">
        {improvements.map((imp) => (
          <div
            key={imp.id}
            className={cn(
              "flex items-center gap-3 px-4 py-3 rounded-xl border group transition-colors",
              priorityColor[imp.priority]
            )}
          >
            <div className="flex-1 min-w-0">
              <p className="text-sm font-medium text-slate-200">{imp.title}</p>
              <p className="text-xs text-slate-400 mt-0.5">{imp.description}</p>
              <p className="text-[11px] text-slate-400/60 mt-1 flex items-center gap-1">
                <TrendingUp className="w-3 h-3" /> {imp.impact}
              </p>
            </div>
            <button
              disabled={runningId !== null}
              onClick={async () => {
                setRunningId(imp.id);
                try {
                  await onImprove(imp.id);
                } finally {
                  setRunningId(null);
                }
              }}
              className={cn(
                "px-3 py-1.5 rounded-lg text-xs font-medium transition-all whitespace-nowrap",
                "bg-white/[0.06] hover:bg-white/[0.12] text-slate-200",
                "opacity-80 group-hover:opacity-100",
                "disabled:opacity-40 disabled:cursor-not-allowed"
              )}
            >
              {runningId === imp.id ? "Running…" : "Fix this"}
            </button>
          </div>
        ))}
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Pipeline stage indicator
// ---------------------------------------------------------------------------

const PIPELINE_STAGES = [
  { label: "Analyze", threshold: 0, icon: Eye },
  { label: "Design", threshold: 20, icon: Palette },
  { label: "Generate", threshold: 40, icon: Code2 },
  { label: "Quality", threshold: 75, icon: Sparkles },
  { label: "Done", threshold: 100, icon: CheckCircle2 },
];

function PipelineStages({ progress }: { progress: number }) {
  return (
    <div className="flex items-center gap-1 w-full max-w-md">
      {PIPELINE_STAGES.map((stage, i) => {
        const isActive = progress >= stage.threshold && (i === PIPELINE_STAGES.length - 1 || progress < PIPELINE_STAGES[i + 1].threshold);
        const isDone = i < PIPELINE_STAGES.length - 1 && progress >= PIPELINE_STAGES[i + 1].threshold;
        const Icon = stage.icon;
        return (
          <div key={stage.label} className="flex items-center gap-1 flex-1">
            <div className={cn(
              "flex items-center gap-1.5 px-2 py-1 rounded-md text-[11px] font-medium transition-all duration-300 whitespace-nowrap",
              isDone
                ? "bg-emerald-400/10 text-emerald-400"
                : isActive
                ? "bg-orange-400/10 text-orange-400 ring-1 ring-orange-400/30"
                : "bg-white/[0.03] text-slate-500",
            )}>
              <Icon className={cn("w-3 h-3 flex-shrink-0", isActive && "animate-pulse")} />
              {stage.label}
            </div>
            {i < PIPELINE_STAGES.length - 1 && (
              <div className={cn(
                "flex-1 h-px min-w-[8px]",
                isDone ? "bg-emerald-400/30" : "bg-white/[0.06]",
              )} />
            )}
          </div>
        );
      })}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Main NexusThinking component
// ---------------------------------------------------------------------------

interface NexusThinkingProps {
  steps: ThinkingStep[];
  progress: number;
  isComplete: boolean;
  isStreaming: boolean;
  onOpenProject?: (projectId: string) => void;
  onOverrideDecision?: (area: string, newValue: string) => void;
  onImprove?: (improvementId: string) => void;
  onPreview?: (projectId: string) => void | Promise<void>;
  onFixWithAi?: (step: ThinkingStep) => void | Promise<void>;
}

export function NexusThinking({
  steps,
  progress,
  isComplete,
  isStreaming,
  onOpenProject,
  onOverrideDecision,
  onImprove,
  onPreview,
  onFixWithAi,
}: NexusThinkingProps) {
  const [previewLoading, setPreviewLoading] = useState(false);
  const scrollRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    }
  }, [steps.length]);

  const completeStep = steps.find((s) => s.type === "complete");

  const fileSteps = steps.filter((s) => s.type === "file");
  const visibleSteps = steps.filter((s) => s.type !== "file" || fileSteps.indexOf(s) >= fileSteps.length - 3);

  return (
    <div className="w-full max-w-2xl mx-auto">
      {/* Progress ring + pipeline */}
      {!isComplete && (
        <div className="flex flex-col items-center mb-6 gap-4">
          <ProgressRing progress={progress} />
          <PipelineStages progress={progress} />
        </div>
      )}

      {/* Thinking stream */}
      <div
        ref={scrollRef}
        className="space-y-2 max-h-[50vh] overflow-y-auto scrollbar-thin pr-1"
      >
        {/* File counter if there are many file events */}
        {fileSteps.length > 3 && !isComplete && (
          <div className="flex items-center gap-2 px-4 py-2 rounded-lg bg-white/[0.02] border border-white/[0.05]">
            <FileCode className="w-3.5 h-3.5 text-emerald-400" />
            <span className="text-xs text-slate-400">
              {fileSteps.length} files written
            </span>
            <div className="flex-1" />
            <div className="flex -space-x-1">
              {fileSteps.slice(-5).map((f) => (
                <div key={f.id} className="w-1.5 h-1.5 rounded-full bg-emerald-400/60" />
              ))}
            </div>
          </div>
        )}
        {visibleSteps.map((step, i) => {
          const isLast = i === visibleSteps.length - 1 && isStreaming;
          if (step.type === "decision") {
            return (
              <DecisionBubble
                key={step.id}
                step={step}
                isLatest={isLast}
                onOverride={onOverrideDecision}
              />
            );
          }
          return <StepBubble key={step.id} step={step} isLatest={isLast} onFixWithAi={onFixWithAi} />;
        })}
        {isStreaming && !isComplete && <ThinkingDots />}
      </div>

      {/* ═══ COMPLETE: Live result experience ═══ */}
      {isComplete && completeStep && (
        <div className="mt-4 space-y-4 fade-in-up">
          {/* Action buttons */}
          <div className="flex items-center gap-3">
            <button
              onClick={() => onOpenProject?.(completeStep.projectId!)}
              className={cn(
                "flex-1 flex items-center justify-center gap-2 px-5 py-3 rounded-xl font-medium text-sm",
                "bg-gradient-to-r from-glow-cyan to-glow-blue text-white",
                "hover:brightness-110 transition-all neon-glow"
              )}
            >
              <Monitor className="w-4 h-4" />
              Open in workspace
            </button>
            <button
              disabled={previewLoading || !onPreview || !completeStep.projectId}
              onClick={async () => {
                if (!onPreview || !completeStep.projectId) return;
                setPreviewLoading(true);
                try {
                  await onPreview(completeStep.projectId);
                } finally {
                  setPreviewLoading(false);
                }
              }}
              className={cn(
                "flex items-center gap-2 px-5 py-3 rounded-xl font-medium text-sm",
                "glass-card glass-card-hover text-slate-200",
                "disabled:opacity-50 disabled:cursor-not-allowed"
              )}
            >
              <ExternalLink className="w-4 h-4" />
              {previewLoading ? "Starting…" : "Preview"}
            </button>
          </div>

          {/* Proactive improvement suggestions */}
          <ImprovementsSuggestion
            tasteScore={completeStep.tasteScore}
            onImprove={onImprove || (() => {})}
          />
        </div>
      )}
    </div>
  );
}
