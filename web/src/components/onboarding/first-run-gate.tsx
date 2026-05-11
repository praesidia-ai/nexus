"use client";

import { useCallback, useEffect, useRef, useState } from "react";
import Link from "next/link";
import { Check, Copy, KeyRound, Laptop, Sparkles, X } from "lucide-react";
import { cn, safeCopyToClipboard } from "@/lib/utils";;
import { useLlmKeyStatus } from "@/hooks/use-llm-key-status";
import { useProjects } from "@/hooks/api/use-projects";

const SKIP_STORAGE_KEY = "nexus.skipped-onboarding";
const OLLAMA_COMMAND = "docker compose up ollama";

// Dispatched on the window whenever the gate is forcibly opened as a modal
// (e.g. when the user tries to submit a prompt without a key configured).
const OPEN_AS_MODAL_EVENT = "nexus:open-first-run-gate";

/**
 * Imperatively open the gate as a focus-trapped modal from anywhere in the
 * tree. Used by the home page when the user presses "Send" without an LLM
 * key configured.
 */
export function openFirstRunGateAsModal(): void {
  if (typeof window === "undefined") return;
  window.dispatchEvent(new Event(OPEN_AS_MODAL_EVENT));
}

interface OptionCardProps {
  icon: React.ComponentType<{ className?: string }>;
  title: string;
  description: string;
  href?: string;
  footer?: React.ReactNode;
  accent: string;
  onSelect?: () => void;
}

function OptionCard({ icon: Icon, title, description, href, footer, accent, onSelect }: OptionCardProps) {
  const body = (
    <>
      <div className="flex items-start gap-3">
        <div className={cn("rounded-lg p-2 flex items-center justify-center flex-shrink-0", accent)}>
          <Icon className="w-4 h-4 text-white/90" />
        </div>
        <div className="min-w-0 flex-1">
          <div className="text-sm font-semibold text-slate-100">{title}</div>
          <p className="text-xs text-slate-400 mt-0.5 leading-relaxed">{description}</p>
          {footer && <div className="mt-2">{footer}</div>}
        </div>
      </div>
    </>
  );

  const className = cn(
    "glass-card glass-card-hover block w-full text-left p-4 transition-all duration-150",
    "focus-visible:outline-none focus-visible:border-glow-cyan/30 focus-visible:shadow-glow-sm",
    "active:scale-[0.99]",
  );

  if (href) {
    return (
      <Link href={href} className={className} onClick={onSelect}>
        {body}
      </Link>
    );
  }

  return (
    <button type="button" className={className} onClick={onSelect}>
      {body}
    </button>
  );
}

function OllamaCommand() {
  const [copied, setCopied] = useState(false);

  const handleCopy = useCallback(async () => {
    try {
      await safeCopyToClipboard(OLLAMA_COMMAND);
      setCopied(true);
      const t = window.setTimeout(() => setCopied(false), 1600);
      return () => window.clearTimeout(t);
    } catch {
      // Clipboard can reject in insecure contexts — silently ignore, the
      // command is still readable on-screen.
    }
  }, []);

  return (
    <div className="flex items-center gap-2 rounded-md border border-white/[0.08] bg-black/30 px-2 py-1.5 font-mono text-[11px] text-slate-200">
      <code className="flex-1 truncate">{OLLAMA_COMMAND}</code>
      <button
        type="button"
        onClick={(e) => {
          e.preventDefault();
          e.stopPropagation();
          void handleCopy();
        }}
        aria-label={copied ? "Copied" : "Copy command"}
        className="inline-flex items-center justify-center w-6 h-6 rounded hover:bg-white/[0.06] text-slate-300"
      >
        {copied ? <Check className="w-3 h-3 text-emerald-400" /> : <Copy className="w-3 h-3" />}
      </button>
    </div>
  );
}

interface GateCardProps {
  modal: boolean;
  onSkip: () => void;
  onClose?: () => void;
}

function GateCard({ modal, onSkip, onClose }: GateCardProps) {
  return (
    <section
      role={modal ? "dialog" : "region"}
      aria-modal={modal ? true : undefined}
      aria-labelledby="first-run-gate-title"
      aria-describedby="first-run-gate-description"
      className={cn(
        "glass-card relative w-full max-w-xl mx-auto p-6",
        "border border-white/[0.08] rounded-[var(--radius)]",
        "shadow-[0_0_40px_rgba(0,212,255,0.06)]",
      )}
    >
      {modal && onClose && (
        <button
          type="button"
          onClick={onClose}
          aria-label="Close onboarding dialog"
          className="absolute right-3 top-3 rounded-md p-1 text-slate-400 hover:text-slate-200 hover:bg-white/[0.06] focus-visible:outline-none focus-visible:shadow-glow-sm"
        >
          <X className="w-4 h-4" />
        </button>
      )}

      <div className="flex items-center gap-2 mb-1">
        <Sparkles className="w-4 h-4 text-glow-cyan" />
        <span className="text-[11px] uppercase tracking-wider text-glow-cyan/80 font-medium">
          Get started
        </span>
      </div>
      <h2 id="first-run-gate-title" className="text-xl font-semibold text-slate-100">
        One more step — connect an AI model
      </h2>
      <p
        id="first-run-gate-description"
        className="text-sm text-slate-400 mt-1.5 leading-relaxed"
      >
        Nexus needs a provider before it can generate. Pick whichever is easiest
        for you — you can change this any time in Settings.
      </p>

      <div className="mt-5 flex flex-col gap-3">
        <OptionCard
          icon={KeyRound}
          title="Paste OpenAI key"
          description="Use GPT-4o or any OpenAI model. Best default — fastest to try."
          href="/settings"
          accent="bg-emerald-500/15"
          onSelect={onClose}
          footer={<span className="text-[11px] text-slate-500">Opens Settings</span>}
        />

        <OptionCard
          icon={KeyRound}
          title="Paste Anthropic key"
          description="Use Claude Sonnet or Opus. Great for long context and polish."
          href="/settings"
          accent="bg-violet-500/15"
          onSelect={onClose}
        />

        <OptionCard
          icon={Laptop}
          title="Run locally with Ollama"
          description="No account required. Run models on your machine via Docker."
          accent="bg-sky-500/15"
          footer={<OllamaCommand />}
        />
      </div>

      <div className="mt-5 flex items-center justify-between">
        <button
          type="button"
          onClick={onSkip}
          className="text-xs text-slate-400 hover:text-slate-200 underline underline-offset-4 focus-visible:outline-none focus-visible:text-slate-200"
        >
          Skip — I&apos;ll explore first
        </button>
        <span className="text-[10px] text-slate-500/80">
          You can re-open this from Settings
        </span>
      </div>
    </section>
  );
}

/**
 * `FirstRunGate` is rendered at the top of the home page and only shows when:
 *   - no LLM key is configured
 *   - Ollama is not reachable
 *   - the user has no projects yet
 *   - the user has NOT dismissed onboarding via localStorage
 *
 * It also listens for a `nexus:open-first-run-gate` window event so the
 * home page can surface it as a modal on first Send attempt even when keys
 * are missing.
 */
export function FirstRunGate() {
  const { hasAnyKey, ollamaAvailable, isLoading } = useLlmKeyStatus();
  const { data: projects = [] } = useProjects();

  const [skipped, setSkipped] = useState<boolean>(false);
  const [modalOpen, setModalOpen] = useState<boolean>(false);
  const [mounted, setMounted] = useState<boolean>(false);

  const modalRef = useRef<HTMLDivElement | null>(null);
  const previouslyFocused = useRef<HTMLElement | null>(null);

  useEffect(() => {
    setMounted(true);
    if (typeof window !== "undefined") {
      setSkipped(window.localStorage.getItem(SKIP_STORAGE_KEY) === "1");
    }
  }, []);

  // Listen for external open requests (e.g. Send without a key).
  useEffect(() => {
    if (typeof window === "undefined") return;
    const handler = () => setModalOpen(true);
    window.addEventListener(OPEN_AS_MODAL_EVENT, handler);
    return () => window.removeEventListener(OPEN_AS_MODAL_EVENT, handler);
  }, []);

  const handleSkip = useCallback(() => {
    if (typeof window !== "undefined") {
      window.localStorage.setItem(SKIP_STORAGE_KEY, "1");
    }
    setSkipped(true);
    setModalOpen(false);
  }, []);

  const handleClose = useCallback(() => {
    setModalOpen(false);
  }, []);

  // Focus trap + Escape handling when rendered as a modal.
  useEffect(() => {
    if (!modalOpen) return;

    previouslyFocused.current = (document.activeElement as HTMLElement | null) ?? null;
    // Defer to allow the dialog to mount before we move focus.
    const focusTimer = window.setTimeout(() => {
      modalRef.current?.focus();
    }, 0);

    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        setModalOpen(false);
        return;
      }
      if (e.key !== "Tab") return;

      const root = modalRef.current;
      if (!root) return;
      const focusables = root.querySelectorAll<HTMLElement>(
        'a[href], button:not([disabled]), [tabindex]:not([tabindex="-1"])',
      );
      if (focusables.length === 0) return;
      const first = focusables[0];
      const last = focusables[focusables.length - 1];
      const active = document.activeElement as HTMLElement | null;
      if (e.shiftKey) {
        if (active === first || !root.contains(active)) {
          e.preventDefault();
          last.focus();
        }
      } else if (active === last) {
        e.preventDefault();
        first.focus();
      }
    };

    document.addEventListener("keydown", onKeyDown);
    return () => {
      document.removeEventListener("keydown", onKeyDown);
      window.clearTimeout(focusTimer);
      previouslyFocused.current?.focus?.();
    };
  }, [modalOpen]);

  if (!mounted) return null;

  const shouldShow =
    !isLoading && !hasAnyKey && !ollamaAvailable && projects.length === 0 && !skipped;

  // Modal path: surfaced when the user tries to generate without a key. We
  // still render it even if the inline gate is hidden (e.g. they had a
  // project), so it acts as a guard.
  if (modalOpen) {
    return (
      <div
        className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm p-4"
        onClick={(e) => {
          if (e.target === e.currentTarget) setModalOpen(false);
        }}
      >
        <div
          ref={modalRef}
          tabIndex={-1}
          className="outline-none w-full max-w-xl"
        >
          <GateCard
            modal
            onSkip={handleSkip}
            onClose={handleClose}
          />
        </div>
      </div>
    );
  }

  if (!shouldShow) return null;

  return (
    <div className="w-full max-w-xl mx-auto mb-6">
      <GateCard modal={false} onSkip={handleSkip} />
    </div>
  );
}
