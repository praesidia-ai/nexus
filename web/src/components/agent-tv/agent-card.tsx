"use client";

import { useEffect, useRef } from "react";
import { cn } from "@/lib/utils";
import type {
  AgentCard as AgentCardMeta,
  AgentStatus,
  AgentStatusSnapshot,
  FileLine,
} from "@/lib/agent-tv-types";

interface AgentCardProps {
  agent: AgentCardMeta;
  status: AgentStatusSnapshot;
  thoughts: string;
  files: FileLine[];
}

/** Status pill colour + copy metadata. */
const STATUS_META: Record<
  AgentStatus,
  { label: string; dotClass: string; pillClass: string; pulse: boolean }
> = {
  idle: {
    label: "Idle",
    dotClass: "bg-slate-500",
    pillClass: "bg-slate-500/10 text-slate-300 border-slate-500/20",
    pulse: false,
  },
  thinking: {
    label: "Thinking",
    dotClass: "bg-amber-400",
    pillClass: "bg-amber-500/10 text-amber-300 border-amber-500/30",
    pulse: true,
  },
  writing: {
    label: "Writing",
    dotClass: "bg-cyan-400",
    pillClass: "bg-cyan-500/10 text-cyan-300 border-cyan-500/30",
    pulse: true,
  },
  reviewing: {
    label: "Reviewing",
    dotClass: "bg-violet-400",
    pillClass: "bg-violet-500/10 text-violet-300 border-violet-500/30",
    pulse: false,
  },
  done: {
    label: "Done",
    dotClass: "bg-emerald-400",
    pillClass: "bg-emerald-500/10 text-emerald-300 border-emerald-500/30",
    pulse: false,
  },
  error: {
    label: "Error",
    dotClass: "bg-red-400",
    pillClass: "bg-red-500/10 text-red-300 border-red-500/30",
    pulse: false,
  },
};

/** Format a cumulative cost for the live ticker (4 decimals once it crosses a cent). */
function formatCost(usd: number): string {
  if (usd >= 1) return `$${usd.toFixed(2)}`;
  if (usd >= 0.01) return `$${usd.toFixed(4)}`;
  return `$${usd.toFixed(4)}`;
}

/** Human-friendly token count: `1234` → `1.2k`. */
function formatTokens(n: number): string {
  if (n < 1000) return n.toString();
  const k = n / 1000;
  return `${k.toFixed(k >= 10 ? 0 : 1)}k`;
}

/** Keep only the last N lines of streamed thoughts for the card tail view. */
function tailLines(text: string, maxLines: number): string {
  if (!text) return "";
  const lines = text.split("\n");
  if (lines.length <= maxLines) return text;
  return lines.slice(lines.length - maxLines).join("\n");
}

function truncateMiddle(path: string, maxLen = 42): string {
  if (path.length <= maxLen) return path;
  const keep = Math.floor((maxLen - 1) / 2);
  return `${path.slice(0, keep)}…${path.slice(path.length - keep)}`;
}

export function AgentCard({ agent, status, thoughts, files }: AgentCardProps) {
  const thoughtRef = useRef<HTMLDivElement | null>(null);
  const meta = STATUS_META[status.status];
  const isActive = status.status === "thinking" || status.status === "writing";
  const errored = status.status === "error";
  const tail = tailLines(thoughts, 6);

  // Auto-scroll thoughts box as new chunks arrive.
  useEffect(() => {
    const el = thoughtRef.current;
    if (!el) return;
    el.scrollTop = el.scrollHeight;
  }, [thoughts]);

  const lastFile = files[0] ?? null;

  return (
    <article
      tabIndex={0}
      aria-label={`Agent ${agent.name}, ${meta.label}`}
      className={cn(
        "glass-card group relative flex h-full flex-col gap-3 p-4",
        "outline-none transition-all duration-200",
        "focus-visible:ring-2 focus-visible:ring-cyan-400/60 focus-visible:ring-offset-0",
        "glass-card-hover",
        isActive && "neon-glow",
        status.recentlyWrote && "neon-glow ring-1 ring-cyan-400/40",
        errored && "ring-1 ring-red-500/40",
      )}
    >
      {/* Header row: emoji avatar + name/role + status pill */}
      <div className="flex items-start gap-3">
        <div
          className={cn(
            "flex h-12 w-12 shrink-0 items-center justify-center rounded-xl",
            "bg-white/5 text-[28px] leading-none",
            "border border-white/10",
            isActive && "animate-pulse",
          )}
          aria-hidden="true"
        >
          <span>{agent.emoji}</span>
        </div>
        <div className="min-w-0 flex-1">
          <div className="flex items-center justify-between gap-2">
            <h3 className="truncate text-sm font-semibold text-slate-100">
              {agent.name}
            </h3>
            <span
              className={cn(
                "inline-flex shrink-0 items-center gap-1.5 rounded-full border px-2 py-0.5 text-[10px] font-medium uppercase tracking-wide",
                meta.pillClass,
              )}
            >
              <span
                className={cn(
                  "h-1.5 w-1.5 rounded-full",
                  meta.dotClass,
                  meta.pulse && "animate-pulse",
                )}
                aria-hidden="true"
              />
              {meta.label}
            </span>
          </div>
          <p className="mt-0.5 truncate text-[11px] text-slate-400">
            {agent.role}
          </p>
        </div>
      </div>

      {/* Current file + tool badge */}
      <div className="flex min-h-[20px] items-center justify-between gap-2 text-[11px]">
        <span
          className="min-w-0 flex-1 truncate font-mono text-slate-300"
          title={status.currentFile ?? ""}
        >
          {status.currentFile
            ? truncateMiddle(status.currentFile)
            : <span className="text-slate-500">—</span>}
        </span>
        {status.lastTool && (
          <span
            className="shrink-0 rounded border border-cyan-500/30 bg-cyan-500/10 px-1.5 py-0.5 font-mono text-[10px] text-cyan-200"
            aria-label={`Tool ${status.lastTool}`}
          >
            {status.lastTool}
          </span>
        )}
      </div>

      {/* Cost + tokens ticker */}
      <div className="flex items-center justify-between gap-2 text-[11px] font-mono">
        <span className="text-cyan-300" aria-label={`Cost ${status.costUsd}`}>
          {formatCost(status.costUsd)}
        </span>
        <span className="text-slate-400" aria-label="Tokens">
          {formatTokens(status.tokensIn)} in /{" "}
          {formatTokens(status.tokensOut)} out
        </span>
      </div>

      {/* Thoughts stream — aria-live announces new chunks to assistive tech. */}
      <div
        ref={thoughtRef}
        role="log"
        aria-live="polite"
        aria-label={`${agent.name} thoughts`}
        className={cn(
          "scrollbar-thin mt-auto max-h-32 min-h-[80px] overflow-y-auto",
          "rounded-lg border border-white/5 bg-black/30 p-2",
          "font-mono text-[11px] leading-relaxed text-slate-300",
          "whitespace-pre-wrap break-words",
        )}
      >
        {tail ? (
          tail
        ) : (
          <span className="italic text-slate-500">
            Waiting for output…
          </span>
        )}
      </div>

      {/* Most recent file write — compact footer */}
      {lastFile && (
        <div
          className="flex items-center gap-2 text-[10px] font-mono text-slate-400"
          title={lastFile.path}
        >
          <span
            className={cn(
              "rounded px-1.5 py-0.5 uppercase tracking-wide",
              lastFile.kind === "created"
                ? "bg-emerald-500/10 text-emerald-300"
                : "bg-amber-500/10 text-amber-300",
            )}
          >
            {lastFile.kind}
          </span>
          <span className="min-w-0 flex-1 truncate">
            {truncateMiddle(lastFile.path)}
          </span>
          <span className="shrink-0 text-slate-500">
            {lastFile.lines}L
          </span>
        </div>
      )}
    </article>
  );
}
