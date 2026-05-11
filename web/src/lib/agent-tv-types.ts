/**
 * Frontend mirror of the Rust `AgentTvEvent` tagged union defined in
 * `docs/AGENT_TV_CONTRACT.md`. These types MUST stay byte-compatible with the
 * backend's `#[serde(tag = "type", rename_all = "snake_case")]` payloads.
 */

export type AgentStatus =
  | "idle"
  | "thinking"
  | "writing"
  | "reviewing"
  | "done"
  | "error";

export type FileWriteKind = "created" | "updated";

export interface AgentCard {
  id: string;
  name: string;
  emoji: string;
  role: string;
  status: AgentStatus;
}

export type AgentTvEvent =
  | { type: "roster_update"; agents: AgentCard[] }
  | {
      type: "agent_status";
      agent_id: string;
      status: AgentStatus;
      current_file: string | null;
      cost_usd: number;
      tokens_in: number;
      tokens_out: number;
    }
  | {
      type: "agent_thought";
      agent_id: string;
      content: string;
      timestamp_ms: number;
    }
  | {
      type: "agent_file_write";
      agent_id: string;
      path: string;
      lines: number;
      kind: FileWriteKind;
    }
  | {
      type: "agent_tool_call";
      agent_id: string;
      tool: string;
      args_preview: string;
    }
  | { type: "heartbeat"; elapsed_ms: number }
  | {
      type: "complete";
      total_cost_usd: number;
      total_files: number;
      duration_ms: number;
      agents_used: string[];
    }
  | { type: "error"; message: string };

/** Snapshot of the most recent AgentStatus event per agent. */
export interface AgentStatusSnapshot {
  status: AgentStatus;
  currentFile: string | null;
  costUsd: number;
  tokensIn: number;
  tokensOut: number;
  /** Brief tool badge flashed when an AgentToolCall arrives. Cleared by timers. */
  lastTool: string | null;
  /** True for a short window after a file write — used to pulse the card. */
  recentlyWrote: boolean;
}

/** Per-agent file write record. */
export interface FileLine {
  path: string;
  lines: number;
  kind: FileWriteKind;
  timestampMs: number;
}

export interface AgentTvState {
  roster: AgentCard[];
  statusByAgent: Record<string, AgentStatusSnapshot>;
  thoughtsByAgent: Record<string, string>;
  filesByAgent: Record<string, FileLine[]>;
  totalCostUsd: number;
  totalFiles: number;
  complete: boolean;
  completeSummary: {
    totalCostUsd: number;
    totalFiles: number;
    durationMs: number;
    agentsUsed: string[];
  } | null;
  error: string | null;
  elapsedMs: number;
  connectionStatus: "connecting" | "connected" | "disconnected" | "error";
  /**
   * Cumulative count of events the server told us it dropped because the
   * broadcast channel fell behind. Non-zero means the UI is momentarily
   * out of sync — show the "catching up" banner until a fresh event arrives.
   */
  lagEventCount: number;
  /** Total events reported as skipped across all lag frames. */
  lagSkippedTotal: number;
}

/** Default per-agent snapshot used when we see a status event before roster. */
export const DEFAULT_AGENT_STATUS: AgentStatusSnapshot = {
  status: "idle",
  currentFile: null,
  costUsd: 0,
  tokensIn: 0,
  tokensOut: 0,
  lastTool: null,
  recentlyWrote: false,
};
