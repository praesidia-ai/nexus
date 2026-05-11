"use client";

import { useRef, useState, useEffect, useCallback } from "react";
import {
  Send, Globe, Briefcase, Home, ShoppingCart, Rocket, Code2,
  BarChart2, Users, Layers, Cpu, Package, Megaphone, WifiOff, RefreshCw,
  LayoutDashboard, ShoppingBag, MessageSquare, PenSquare, Settings2,
  Sparkles, Gamepad2,
} from "lucide-react";
import Link from "next/link";
import { cn } from "@/lib/utils";
import { GlowButton } from "@/components/ui/glow-button";
import { NexusLogo } from "@/components/brand/nexus-logo";
import type { Project } from "@/lib/api";

const ALL_SUGGESTIONS = [
  { icon: Globe, label: "Build a translation agency" },
  { icon: Briefcase, label: "Launch a consulting practice" },
  { icon: Home, label: "Manage rental properties" },
  { icon: ShoppingCart, label: "Start an e-commerce store" },
  { icon: Rocket, label: "Ship a SaaS product" },
  { icon: Code2, label: "Automate a dev workflow" },
  { icon: BarChart2, label: "Track sales pipeline" },
  { icon: Users, label: "Build a hiring process" },
  { icon: Layers, label: "Design a service business" },
  { icon: Cpu, label: "Launch an AI assistant" },
  { icon: Package, label: "Manage inventory & orders" },
  { icon: Megaphone, label: "Run a marketing campaign" },
];

interface StarterTemplate {
  id: string;
  title: string;
  tagline: string;
  prompt: string;
  icon: React.ComponentType<{ className?: string }>;
  accent: string; // tailwind gradient classes
}

const STARTER_TEMPLATES: StarterTemplate[] = [
  {
    id: "saas-analytics",
    title: "Analytics Cockpit",
    tagline: "SaaS dashboard with live KPIs",
    prompt:
      "Build a SaaS analytics dashboard with a sidebar nav, top KPI cards (MRR, active users, churn), a revenue line chart, and a recent signups table. Clean light theme with a dark sidebar and a date-range switcher in the header.",
    icon: LayoutDashboard,
    accent: "from-violet-500/30 to-indigo-500/20 border-violet-400/20",
  },
  {
    id: "landing-page",
    title: "Startup Launch",
    tagline: "Conversion landing page",
    prompt:
      "Create a modern landing page for a productivity startup with a bold hero, animated gradient background, feature grid with icons, testimonials carousel, pricing tiers, and a waitlist email form. Mobile-first with subtle scroll animations.",
    icon: Rocket,
    accent: "from-orange-500/30 to-pink-500/20 border-orange-400/20",
  },
  {
    id: "ecommerce",
    title: "Boutique Store",
    tagline: "Storefront with cart & checkout",
    prompt:
      "Build an e-commerce store for a curated coffee brand with a product grid, product detail pages, a slide-out cart, and a checkout form. Include category filters, search, and a warm editorial aesthetic.",
    icon: ShoppingBag,
    accent: "from-amber-500/30 to-rose-500/20 border-amber-400/20",
  },
  {
    id: "social-chat",
    title: "Team Chat",
    tagline: "Slack-style workspace",
    prompt:
      "Create a real-time team chat app with channels, direct messages, threaded replies, emoji reactions, and user presence indicators. Include a left channel list, message pane, and right-side member panel.",
    icon: MessageSquare,
    accent: "from-sky-500/30 to-cyan-500/20 border-sky-400/20",
  },
  {
    id: "portfolio",
    title: "Designer Portfolio",
    tagline: "Portfolio + blog",
    prompt:
      "Design a personal portfolio site for a product designer with a hero intro, selected works grid with hover previews, about section, and a blog. Use big typography and generous whitespace.",
    icon: PenSquare,
    accent: "from-stone-500/30 to-neutral-500/20 border-stone-400/20",
  },
  {
    id: "internal-admin",
    title: "Ops Console",
    tagline: "Internal admin tool",
    prompt:
      "Build an internal admin panel with a users table (search, filter, pagination), a detail drawer to edit roles and status, an orders view with status badges, and an audit log timeline. Dense, keyboard-friendly UI.",
    icon: Settings2,
    accent: "from-slate-500/30 to-zinc-500/20 border-slate-400/20",
  },
  {
    id: "ai-rag",
    title: "Docs Copilot",
    tagline: "RAG chat on your docs",
    prompt:
      "Create an AI documentation assistant with a chat interface, streaming responses, source citations as inline pills, a document upload sidebar, and a conversation history list. Include a 'thinking' state and suggested follow-up prompts.",
    icon: Sparkles,
    accent: "from-fuchsia-500/30 to-purple-500/20 border-fuchsia-400/20",
  },
  {
    id: "game",
    title: "Memory Match",
    tagline: "Playful card game",
    prompt:
      "Build a memory card-matching game with a 4x4 grid of flippable emoji cards, a move counter, timer, win modal with confetti, and a difficulty selector. Juicy animations on flip and match.",
    icon: Gamepad2,
    accent: "from-emerald-400/30 to-teal-500/20 border-emerald-400/20",
  },
];

function pickRandom<T>(arr: T[], n: number): T[] {
  const copy = [...arr];
  const result: T[] = [];
  for (let i = 0; i < n && copy.length > 0; i++) {
    const idx = Math.floor(Math.random() * copy.length);
    result.push(copy.splice(idx, 1)[0]);
  }
  return result;
}

const HEADINGS = [
  "What are you working on?",
  "What shall we build today?",
  "What\u2019s the next big thing?",
  "What problem can we solve?",
  "Ready to build something great?",
];

const SUBHEADINGS = [
  "Describe your idea. Nexus handles the rest.",
  "One sentence. Full application. Watch it think.",
  "Type anything. Watch intelligence at work.",
];

interface GenerationInputProps {
  input: string;
  onInputChange: (value: string) => void;
  onSubmit: (text: string) => void;
  onRetryConnection?: () => void;
  backendOnline: boolean;
  projects: Project[];
  hasError?: boolean;
}

export function GenerationInput({
  input,
  onInputChange,
  onSubmit,
  onRetryConnection,
  backendOnline,
  projects,
  hasError,
}: GenerationInputProps) {
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  // Avoid hydration mismatch: randomize only on client
  const [heading, setHeading] = useState(HEADINGS[0]);
  const [subheading, setSubheading] = useState(SUBHEADINGS[0]);
  const [suggestions, setSuggestions] = useState(ALL_SUGGESTIONS.slice(0, 4));
  const [mounted, setMounted] = useState(false);

  useEffect(() => {
    setHeading(HEADINGS[Math.floor(Math.random() * HEADINGS.length)]);
    setSubheading(SUBHEADINGS[Math.floor(Math.random() * SUBHEADINGS.length)]);
    setSuggestions(pickRandom(ALL_SUGGESTIONS, 4));
    setMounted(true);
  }, []);

  useEffect(() => {
    if (mounted) textareaRef.current?.focus();
  }, [mounted]);

  const autoResize = useCallback(() => {
    const el = textareaRef.current;
    if (!el) return;
    el.style.height = "auto";
    el.style.height = `${Math.min(el.scrollHeight, 160)}px`;
  }, []);

  useEffect(() => { autoResize(); }, [input, autoResize]);

  function handleKeyDown(e: React.KeyboardEvent) {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      if (input.trim()) onSubmit(input);
    }
  }

  const canSubmit = input.trim().length > 0;

  return (
    <div className={cn("flex flex-col items-center w-full", hasError && "mt-4")}>
      {/* Animated logo hero */}
      <div className="mb-6">
        <NexusLogo size={160} animated />
      </div>

      <h1 className="text-3xl font-bold text-slate-200 mb-2 text-center">{heading}</h1>
      <p className="text-slate-400 text-base mb-10 text-center">{subheading}</p>

      {/* Quick idea chips */}
      <div className="grid grid-cols-2 gap-3 mb-6 w-full max-w-lg">
        {suggestions.map((s, i) => (
          <button
            key={s.label}
            type="button"
            onClick={() => { onInputChange(s.label); onSubmit(s.label); }}
            className={cn(
              "glass-card glass-card-hover cursor-pointer p-4 flex items-center gap-3 text-left",
              "focus-visible:outline-none focus-visible:border-glow-cyan/30 focus-visible:shadow-glow-sm",
              "active:scale-[0.98] transition-all duration-150",
              "fade-in-up",
              `stagger-${i + 1}`,
            )}
          >
            <div className="rounded-lg bg-glow-cyan/[0.08] p-2 flex items-center justify-center flex-shrink-0">
              <s.icon className="w-4 h-4 text-glow-cyan" />
            </div>
            <span className="text-sm text-slate-200">{s.label}</span>
          </button>
        ))}
      </div>

      {/* Starter templates gallery */}
      <div className="w-full max-w-4xl mb-10">
        <div className="flex items-center justify-between mb-3 px-1">
          <p className="text-[11px] uppercase tracking-wider text-slate-500 font-medium">
            Or start from a template
          </p>
          <p className="text-[10px] text-slate-500/70">Click any to fill the prompt</p>
        </div>
        <div className="grid grid-cols-2 md:grid-cols-4 gap-3">
          {STARTER_TEMPLATES.map((t, i) => (
            <button
              key={t.id}
              type="button"
              onClick={() => {
                onInputChange(t.prompt);
                textareaRef.current?.focus();
              }}
              className={cn(
                "group relative overflow-hidden rounded-xl border bg-gradient-to-br",
                "p-3 text-left transition-all duration-200",
                "hover:scale-[1.02] hover:shadow-lg active:scale-[0.99]",
                "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-glow-cyan/40",
                "fade-in-up",
                `stagger-${(i % 4) + 1}`,
                t.accent,
              )}
              aria-label={`Use ${t.title} template`}
            >
              <div className="relative z-10">
                <div className="flex items-center gap-2 mb-1.5">
                  <t.icon className="w-4 h-4 text-white/90 flex-shrink-0" />
                  <span className="text-[13px] font-semibold text-white/95 truncate">
                    {t.title}
                  </span>
                </div>
                <p className="text-[11px] text-white/60 line-clamp-2 leading-snug">
                  {t.tagline}
                </p>
              </div>
              <div className="absolute inset-0 bg-gradient-to-tr from-white/0 via-white/5 to-white/10 opacity-0 group-hover:opacity-100 transition-opacity pointer-events-none" />
            </button>
          ))}
        </div>
      </div>

      {/* Backend offline warning */}
      {!backendOnline && (
        <div className="flex items-center gap-2 px-4 py-2.5 rounded-xl border border-amber-500/20 bg-amber-500/[0.06] text-amber-400 text-sm mb-6 w-full max-w-xl">
          <WifiOff className="w-4 h-4 flex-shrink-0" />
          <span className="flex-1">Backend is not reachable. Make sure the Nexus server is running.</span>
          {onRetryConnection && (
            <button
              type="button"
              onClick={onRetryConnection}
              aria-label="Retry backend connection"
              className="flex items-center gap-1 px-2 py-1 rounded-md text-xs font-medium hover:bg-amber-500/10 transition-colors focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-amber-400/40"
            >
              <RefreshCw className="w-3 h-3" />
              Retry
            </button>
          )}
        </div>
      )}

      {/* Generation textarea */}
      <div
        className={cn(
          "w-full max-w-xl rounded-[var(--radius)] border bg-white/[0.04] backdrop-blur-xl",
          "border-white/[0.08] flex items-end gap-2 px-4 py-3",
          "focus-within:border-glow-cyan/25 focus-within:shadow-glow-sm",
          "transition-all duration-200",
        )}
      >
        <textarea
          ref={textareaRef}
          rows={1}
          value={input}
          onChange={(e) => onInputChange(e.target.value)}
          onKeyDown={handleKeyDown}
          placeholder="Describe the app you want to build..."
          aria-label="Describe the app you want to build"
          className="flex-1 bg-transparent border-0 focus:outline-none text-slate-200 placeholder:text-slate-400/50 text-base resize-none scrollbar-thin leading-relaxed"
        />
        <GlowButton
          variant={canSubmit ? "primary" : "ghost"}
          size="sm"
          onClick={() => { if (canSubmit) onSubmit(input); }}
          disabled={!canSubmit}
          aria-label="Generate"
          className="h-9 w-9 !px-0 flex-shrink-0"
        >
          <Send className="w-4 h-4" />
        </GlowButton>
      </div>

      {/* Quick stats */}
      {projects.length > 0 && (
        <div className="flex items-center gap-6 mt-6 text-xs text-slate-500">
          <span>{projects.length} project{projects.length !== 1 ? "s" : ""}</span>
          <span className="w-px h-3 bg-white/[0.06]" />
          <span>{projects.filter((p) => p.phase >= 4).length} published</span>
        </div>
      )}

      {/* Recent projects */}
      {projects.length > 0 && (
        <div className="w-full max-w-xl mt-4">
          <p className="text-xs text-slate-400/40 mb-2 px-1">Recent projects</p>
          <div className="flex gap-2 overflow-x-auto pb-1 scrollbar-thin">
            {projects.slice(0, 8).map((p) => (
              <Link
                key={p.id}
                href={`/${p.id}`}
                className={cn(
                  "glass-card glass-card-hover px-3 py-2 text-sm flex-shrink-0 text-slate-200 flex items-center gap-2",
                  "focus-visible:outline-none focus-visible:border-glow-cyan/30 focus-visible:shadow-glow-sm",
                  "active:scale-[0.98] transition-all duration-150",
                )}
              >
                <div className="w-5 h-5 rounded bg-glow-cyan/[0.08] flex items-center justify-center flex-shrink-0">
                  <span className="text-glow-cyan text-[10px] font-bold">{p.name.charAt(0).toUpperCase()}</span>
                </div>
                <span className="truncate max-w-[150px]">{p.name}</span>
              </Link>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}
