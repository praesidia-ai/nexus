"use client";

import { useCallback, useEffect, useReducer, useRef } from "react";
import {
  type AgentTvEvent,
  type AgentTvState,
  type AgentStatusSnapshot,
  type FileLine,
  DEFAULT_AGENT_STATUS,
} from "@/lib/agent-tv-types";

/** Reducer actions: either an incoming SSE event or a local transport event. */
type Action =
  | { kind: "event"; event: AgentTvEvent }
  | { kind: "connection"; status: AgentTvState["connectionStatus"] }
  | { kind: "clear-tool"; agentId: string }
  | { kind: "clear-pulse"; agentId: string }
  | { kind: "lag"; skipped: number };

const INITIAL_STATE: AgentTvState = {
  roster: [],
  statusByAgent: {},
  thoughtsByAgent: {},
  filesByAgent: {},
  totalCostUsd: 0,
  totalFiles: 0,
  complete: false,
  completeSummary: null,
  error: null,
  elapsedMs: 0,
  connectionStatus: "connecting",
  lagEventCount: 0,
  lagSkippedTotal: 0,
};

/** Max thought chars kept per agent — the UI only tails the last 6 lines anyway. */
const MAX_THOUGHT_CHARS = 16 * 1024;
/** Max file rows kept per agent, newest first. */
const MAX_FILES_PER_AGENT = 100;

function ensureSnapshot(
  map: Record<string, AgentStatusSnapshot>,
  agentId: string,
): AgentStatusSnapshot {
  return map[agentId] ?? { ...DEFAULT_AGENT_STATUS };
}

function reducer(state: AgentTvState, action: Action): AgentTvState {
  if (action.kind === "connection") {
    return { ...state, connectionStatus: action.status };
  }

  if (action.kind === "clear-tool") {
    const snap = state.statusByAgent[action.agentId];
    if (!snap || snap.lastTool === null) return state;
    return {
      ...state,
      statusByAgent: {
        ...state.statusByAgent,
        [action.agentId]: { ...snap, lastTool: null },
      },
    };
  }

  if (action.kind === "lag") {
    return {
      ...state,
      lagEventCount: state.lagEventCount + 1,
      lagSkippedTotal: state.lagSkippedTotal + action.skipped,
    };
  }

  if (action.kind === "clear-pulse") {
    const snap = state.statusByAgent[action.agentId];
    if (!snap || !snap.recentlyWrote) return state;
    return {
      ...state,
      statusByAgent: {
        ...state.statusByAgent,
        [action.agentId]: { ...snap, recentlyWrote: false },
      },
    };
  }

  const event = action.event;
  switch (event.type) {
    case "roster_update": {
      // Initialise snapshots for any new agents so cards render instantly.
      const nextStatus: Record<string, AgentStatusSnapshot> = {
        ...state.statusByAgent,
      };
      for (const agent of event.agents) {
        if (!nextStatus[agent.id]) {
          nextStatus[agent.id] = { ...DEFAULT_AGENT_STATUS, status: agent.status };
        } else {
          nextStatus[agent.id] = { ...nextStatus[agent.id], status: agent.status };
        }
      }
      return {
        ...state,
        roster: event.agents,
        statusByAgent: nextStatus,
      };
    }

    case "agent_status": {
      const prev = ensureSnapshot(state.statusByAgent, event.agent_id);
      return {
        ...state,
        statusByAgent: {
          ...state.statusByAgent,
          [event.agent_id]: {
            ...prev,
            status: event.status,
            currentFile: event.current_file,
            costUsd: event.cost_usd,
            tokensIn: event.tokens_in,
            tokensOut: event.tokens_out,
          },
        },
      };
    }

    case "agent_thought": {
      const existing = state.thoughtsByAgent[event.agent_id] ?? "";
      const combined = existing + event.content;
      const trimmed =
        combined.length > MAX_THOUGHT_CHARS
          ? combined.slice(combined.length - MAX_THOUGHT_CHARS)
          : combined;
      return {
        ...state,
        thoughtsByAgent: {
          ...state.thoughtsByAgent,
          [event.agent_id]: trimmed,
        },
      };
    }

    case "agent_file_write": {
      const prevFiles = state.filesByAgent[event.agent_id] ?? [];
      const row: FileLine = {
        path: event.path,
        lines: event.lines,
        kind: event.kind,
        timestampMs: Date.now(),
      };
      const nextFiles = [row, ...prevFiles].slice(0, MAX_FILES_PER_AGENT);
      const prevSnap = ensureSnapshot(state.statusByAgent, event.agent_id);
      return {
        ...state,
        filesByAgent: {
          ...state.filesByAgent,
          [event.agent_id]: nextFiles,
        },
        statusByAgent: {
          ...state.statusByAgent,
          [event.agent_id]: {
            ...prevSnap,
            currentFile: event.path,
            recentlyWrote: true,
          },
        },
        totalFiles: state.totalFiles + 1,
      };
    }

    case "agent_tool_call": {
      const prev = ensureSnapshot(state.statusByAgent, event.agent_id);
      return {
        ...state,
        statusByAgent: {
          ...state.statusByAgent,
          [event.agent_id]: { ...prev, lastTool: event.tool },
        },
      };
    }

    case "heartbeat":
      return { ...state, elapsedMs: event.elapsed_ms };

    case "complete": {
      // Cumulative cost is authoritative from backend's Complete event.
      return {
        ...state,
        complete: true,
        totalCostUsd: event.total_cost_usd,
        totalFiles: event.total_files,
        elapsedMs: event.duration_ms,
        completeSummary: {
          totalCostUsd: event.total_cost_usd,
          totalFiles: event.total_files,
          durationMs: event.duration_ms,
          agentsUsed: event.agents_used,
        },
      };
    }

    case "error":
      return { ...state, error: event.message };
  }
}

export interface UseAgentTvResult extends AgentTvState {
  close: () => void;
}

/**
 * Subscribe to the Agent TV SSE stream for a project.
 *
 * Opens `/api/projects/:id/agent-tv/stream` via EventSource, dispatches each
 * typed event into a reducer, and auto-reconnects with exponential backoff
 * (1s → 2s → ... capped at 30s). The caller may explicitly `close()` to
 * tear the connection down early.
 */
export function useAgentTv(projectId: string): UseAgentTvResult {
  const [state, dispatch] = useReducer(reducer, INITIAL_STATE);

  const esRef = useRef<EventSource | null>(null);
  const reconnectTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const reconnectAttemptsRef = useRef(0);
  const closedManuallyRef = useRef(false);
  const toolTimersRef = useRef<Map<string, ReturnType<typeof setTimeout>>>(
    new Map(),
  );
  const pulseTimersRef = useRef<Map<string, ReturnType<typeof setTimeout>>>(
    new Map(),
  );

  const flashToolBadge = useCallback((agentId: string) => {
    const existing = toolTimersRef.current.get(agentId);
    if (existing) clearTimeout(existing);
    const timer = setTimeout(() => {
      dispatch({ kind: "clear-tool", agentId });
      toolTimersRef.current.delete(agentId);
    }, 2000);
    toolTimersRef.current.set(agentId, timer);
  }, []);

  const flashPulse = useCallback((agentId: string) => {
    const existing = pulseTimersRef.current.get(agentId);
    if (existing) clearTimeout(existing);
    const timer = setTimeout(() => {
      dispatch({ kind: "clear-pulse", agentId });
      pulseTimersRef.current.delete(agentId);
    }, 1500);
    pulseTimersRef.current.set(agentId, timer);
  }, []);

  const closeEventSource = useCallback(() => {
    if (esRef.current) {
      esRef.current.close();
      esRef.current = null;
    }
    if (reconnectTimerRef.current) {
      clearTimeout(reconnectTimerRef.current);
      reconnectTimerRef.current = null;
    }
  }, []);

  const close = useCallback(() => {
    closedManuallyRef.current = true;
    closeEventSource();
    dispatch({ kind: "connection", status: "disconnected" });
  }, [closeEventSource]);

  useEffect(() => {
    if (!projectId) return;
    closedManuallyRef.current = false;
    reconnectAttemptsRef.current = 0;

    // Capture ref .current values at effect-start so the cleanup below operates
    // on the same Map instances that this effect populated, not whatever the
    // ref points to at unmount time.
    const toolTimers = toolTimersRef.current;
    const pulseTimers = pulseTimersRef.current;

    const url = `/api/projects/${encodeURIComponent(projectId)}/agent-tv/stream`;

    const connect = () => {
      if (closedManuallyRef.current) return;
      dispatch({ kind: "connection", status: "connecting" });

      const es = new EventSource(url);
      esRef.current = es;

      es.onopen = () => {
        reconnectAttemptsRef.current = 0;
        dispatch({ kind: "connection", status: "connected" });
      };

      es.onmessage = (msg: MessageEvent<string>) => {
        if (!msg.data) return;
        let parsed: AgentTvEvent;
        try {
          parsed = JSON.parse(msg.data) as AgentTvEvent;
        } catch {
          return;
        }
        dispatch({ kind: "event", event: parsed });
        if (parsed.type === "agent_tool_call") {
          flashToolBadge(parsed.agent_id);
        } else if (parsed.type === "agent_file_write") {
          flashPulse(parsed.agent_id);
        } else if (parsed.type === "complete") {
          // Backend is done — close cleanly, don't reconnect.
          closedManuallyRef.current = true;
          closeEventSource();
          dispatch({ kind: "connection", status: "disconnected" });
        }
      };

      // Server-side `lag` frame: named event with `{message, skipped}`.
      // Show a transient "catching up" banner without dropping state.
      es.addEventListener("lag", (raw: Event) => {
        const msg = raw as MessageEvent<string>;
        let skipped = 0;
        try {
          const parsed = JSON.parse(msg.data) as { skipped?: number };
          if (typeof parsed.skipped === "number" && parsed.skipped > 0) {
            skipped = parsed.skipped;
          }
        } catch {
          // tolerate empty / malformed payloads
        }
        dispatch({ kind: "lag", skipped });
      });

      es.onerror = () => {
        // Browser auto-closes on error; fall through to manual backoff.
        if (esRef.current) {
          esRef.current.close();
          esRef.current = null;
        }
        if (closedManuallyRef.current) return;

        const attempt = reconnectAttemptsRef.current;
        const backoffMs = Math.min(1000 * 2 ** attempt, 30_000);
        reconnectAttemptsRef.current = attempt + 1;
        dispatch({ kind: "connection", status: "connecting" });
        reconnectTimerRef.current = setTimeout(connect, backoffMs);
      };
    };

    connect();

    return () => {
      closedManuallyRef.current = true;
      closeEventSource();
      for (const timer of toolTimers.values()) clearTimeout(timer);
      toolTimers.clear();
      for (const timer of pulseTimers.values()) clearTimeout(timer);
      pulseTimers.clear();
    };
    // Intentional: we only want to rebuild the subscription when projectId
    // changes. `closeEventSource`, `flashToolBadge`, `flashPulse` are stable.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [projectId]);

  return { ...state, close };
}
