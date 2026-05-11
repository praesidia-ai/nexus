"use client";

import Link from "next/link";
import { cn, safeHttpUrl } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import type { AgentTvState } from "@/lib/agent-tv-types";

interface SummaryBannerProps {
  state: AgentTvState;
  /** Optional deep-link to the built app preview, shown as a CTA when present. */
  appUrl?: string | null;
}

function formatDuration(ms: number): string {
  if (ms <= 0) return "0s";
  const totalSeconds = Math.round(ms / 1000);
  if (totalSeconds < 60) return `${totalSeconds}s`;
  const minutes = Math.floor(totalSeconds / 60);
  const seconds = totalSeconds % 60;
  return `${minutes}m ${seconds.toString().padStart(2, "0")}s`;
}

function formatCost(usd: number): string {
  if (usd >= 1) return `$${usd.toFixed(2)}`;
  return `$${usd.toFixed(4)}`;
}

export function SummaryBanner({ state, appUrl }: SummaryBannerProps) {
  if (!state.complete || !state.completeSummary) return null;
  const s = state.completeSummary;
  const agentsLabel = s.agentsUsed.length === 1 ? "agent" : "agents";
  const filesLabel = s.totalFiles === 1 ? "file" : "files";
  const safeUrl = safeHttpUrl(appUrl ?? null);

  return (
    <section
      role="status"
      aria-live="polite"
      className={cn(
        "relative overflow-hidden rounded-2xl border border-emerald-400/25",
        "bg-gradient-to-br from-emerald-500/10 via-cyan-500/10 to-violet-500/10",
        "p-6 sm:p-8",
      )}
    >
      <div
        aria-hidden="true"
        className="pointer-events-none absolute inset-0 opacity-50 gradient-bg"
      />
      <div className="relative flex flex-col gap-4 sm:flex-row sm:items-center sm:justify-between">
        <div className="min-w-0">
          <p className="text-xs font-semibold uppercase tracking-[0.22em] text-emerald-300">
            Run complete
          </p>
          <h2 className="mt-2 text-xl font-semibold leading-tight text-slate-100 sm:text-2xl">
            <span className="neon-text">{s.agentsUsed.length}</span>{" "}
            {agentsLabel}
            <span className="text-slate-500"> · </span>
            <span className="text-slate-100">{s.totalFiles}</span>{" "}
            <span className="text-slate-400 font-normal">{filesLabel}</span>
            <span className="text-slate-500"> · </span>
            <span className="text-slate-100">
              {formatDuration(s.durationMs)}
            </span>
            <span className="text-slate-500"> · </span>
            <span className="text-cyan-300">{formatCost(s.totalCostUsd)}</span>
          </h2>
          {s.agentsUsed.length > 0 && (
            <p className="mt-2 font-mono text-[11px] text-slate-400">
              {s.agentsUsed.join(" · ")}
            </p>
          )}
        </div>

        {safeUrl && (
          <Button asChild size="lg" className="shrink-0">
            <Link
              href={safeUrl}
              target="_blank"
              rel="noopener noreferrer"
              aria-label="Open live preview in a new tab"
            >
              Open preview
            </Link>
          </Button>
        )}
      </div>
    </section>
  );
}
