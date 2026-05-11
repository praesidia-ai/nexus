"use client";

import { cn } from "@/lib/utils";
import { AgentCard } from "./agent-card";
import {
  DEFAULT_AGENT_STATUS,
  type AgentTvState,
} from "@/lib/agent-tv-types";

interface AgentGridProps {
  state: AgentTvState;
}

function formatElapsed(ms: number): string {
  if (ms <= 0) return "0s";
  const totalSeconds = Math.floor(ms / 1000);
  const minutes = Math.floor(totalSeconds / 60);
  const seconds = totalSeconds % 60;
  if (minutes === 0) return `${seconds}s`;
  return `${minutes}m ${seconds.toString().padStart(2, "0")}s`;
}

function formatCost(usd: number): string {
  if (usd >= 1) return `$${usd.toFixed(2)}`;
  return `$${usd.toFixed(4)}`;
}

export function AgentGrid({ state }: AgentGridProps) {
  const connectionLabel = (() => {
    switch (state.connectionStatus) {
      case "connected":
        return state.complete ? "Finished" : "Live";
      case "connecting":
        return "Connecting";
      case "disconnected":
        return state.complete ? "Finished" : "Offline";
      case "error":
        return "Error";
    }
  })();

  const liveDotClass =
    state.connectionStatus === "connected" && !state.complete
      ? "bg-emerald-400 animate-pulse"
      : state.connectionStatus === "connecting"
        ? "bg-amber-400 animate-pulse"
        : state.connectionStatus === "error"
          ? "bg-red-400"
          : "bg-slate-500";

  // Stable order: use backend roster order once known; otherwise show a
  // skeleton-friendly empty grid.
  const roster = state.roster;

  return (
    <section
      className={cn(
        "relative rounded-2xl border border-white/10 bg-white/[0.02] p-4 sm:p-6",
        "overflow-hidden",
      )}
      aria-label="Agent TV grid"
    >
      {/* Subtle ambient backdrop */}
      <div
        aria-hidden="true"
        className="pointer-events-none absolute inset-0 opacity-40 gradient-bg"
      />

      <div className="relative">
        {/* Header */}
        <div className="mb-4 flex flex-wrap items-center justify-between gap-3">
          <div className="flex items-center gap-3">
            <h2 className="text-sm font-semibold uppercase tracking-[0.18em] text-slate-300">
              Agent TV
            </h2>
            <span
              className={cn(
                "inline-flex items-center gap-1.5 rounded-full border px-2 py-0.5 text-[10px] font-semibold uppercase tracking-wide",
                state.connectionStatus === "connected" && !state.complete
                  ? "border-emerald-500/30 bg-emerald-500/10 text-emerald-300"
                  : state.connectionStatus === "error"
                    ? "border-red-500/30 bg-red-500/10 text-red-300"
                    : "border-white/10 bg-white/5 text-slate-300",
              )}
              role="status"
              aria-live="polite"
            >
              <span
                className={cn("h-1.5 w-1.5 rounded-full", liveDotClass)}
                aria-hidden="true"
              />
              {connectionLabel}
            </span>
          </div>

          <div className="flex items-center gap-4 text-[11px] font-mono text-slate-300">
            <span aria-label="Elapsed time">
              <span className="text-slate-500">Elapsed</span>{" "}
              <span className="text-slate-100">
                {formatElapsed(state.elapsedMs)}
              </span>
            </span>
            <span aria-label="Total cost">
              <span className="text-slate-500">Cost</span>{" "}
              <span className="text-cyan-300">
                {formatCost(state.totalCostUsd)}
              </span>
            </span>
            <span aria-label="Total files written">
              <span className="text-slate-500">Files</span>{" "}
              <span className="text-slate-100">{state.totalFiles}</span>
            </span>
          </div>
        </div>

        {state.lagEventCount > 0 && !state.complete && (
          <div
            role="status"
            aria-live="polite"
            className={cn(
              "mb-3 flex items-center gap-2 rounded-md border px-2.5 py-1.5",
              "border-amber-500/30 bg-amber-500/5 text-[11px] text-amber-200",
            )}
          >
            <span
              aria-hidden="true"
              className="inline-flex h-1.5 w-1.5 rounded-full bg-amber-400 animate-pulse"
            />
            <span className="font-mono">
              Catching up — {state.lagSkippedTotal} event
              {state.lagSkippedTotal === 1 ? "" : "s"} skipped
              {state.lagEventCount > 1
                ? ` across ${state.lagEventCount} frames`
                : ""}
            </span>
          </div>
        )}

        {/* Grid: 1 col mobile, 2 tablet, 5 desktop → always 10 cards in a 2×5. */}
        <div
          className={cn(
            "grid gap-3 sm:gap-4",
            "grid-cols-1 sm:grid-cols-2 lg:grid-cols-5",
          )}
        >
          {roster.length === 0
            ? Array.from({ length: 10 }).map((_, i) => (
                <div
                  key={`placeholder-${i}`}
                  className="glass-card h-48 animate-pulse opacity-60"
                  aria-hidden="true"
                />
              ))
            : roster.map((agent) => (
                <AgentCard
                  key={agent.id}
                  agent={agent}
                  status={
                    state.statusByAgent[agent.id] ?? {
                      ...DEFAULT_AGENT_STATUS,
                      status: agent.status,
                    }
                  }
                  thoughts={state.thoughtsByAgent[agent.id] ?? ""}
                  files={state.filesByAgent[agent.id] ?? []}
                />
              ))}
        </div>
      </div>
    </section>
  );
}
