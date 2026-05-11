"use client";

import { useEffect, useRef, useState, useCallback } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { api, streamOneshotForProject, type OneShotEvent } from "@/lib/api";
import { eventToStep, type ThinkingStep } from "@/components/nexus-thinking";

export interface OneshotGenerationState {
  active: boolean;
  isComplete: boolean;
  isError: boolean;
  progress: number;
  steps: ThinkingStep[];
  description: string | null;
  completedProjectId: string | null;
  tasteScore: number | null;
  /** Preview URL emitted by the backend's Complete event, e.g. http://localhost:3101. */
  appUrl: string | null;
  startedAt: number | null;
  lastEventAt: number | null;
  /** Abort the live generation stream. */
  abort: () => void;
  /** Whether the stream was explicitly stopped by the user. */
  aborted: boolean;
}

/**
 * Streams /oneshot/start against an existing project.
 *
 * The home page persists the user's prompt as a `user` message inside a fresh
 * conversation when it calls `POST /projects` with a description. This hook
 * reads that message back out, then triggers the stream — no URL param needed.
 */
export function useOneshotGeneration(projectId: string): OneshotGenerationState {
  const queryClient = useQueryClient();

  const [steps, setSteps] = useState<ThinkingStep[]>([]);
  const [progress, setProgress] = useState(0);
  const [isComplete, setIsComplete] = useState(false);
  const [isError, setIsError] = useState(false);
  const [description, setDescription] = useState<string | null>(null);
  const [tasteScore, setTasteScore] = useState<number | null>(null);
  const [appUrl, setAppUrl] = useState<string | null>(null);
  const [active, setActive] = useState(false);
  const [aborted, setAborted] = useState(false);
  const [startedAt, setStartedAt] = useState<number | null>(null);
  const [lastEventAt, setLastEventAt] = useState<number | null>(null);

  const eventIndexRef = useRef(0);
  const abortRef = useRef<AbortController | null>(null);
  const serverProgressRef = useRef(0);
  const tickerRef = useRef<ReturnType<typeof setInterval> | null>(null);

  const stopTicker = useCallback(() => {
    if (tickerRef.current) {
      clearInterval(tickerRef.current);
      tickerRef.current = null;
    }
  }, []);

  useEffect(() => {
    if (!projectId) return;
    const params =
      typeof window !== "undefined"
        ? new URLSearchParams(window.location.search)
        : new URLSearchParams();
    if (params.get("stream") !== "1") return;

    const abort = new AbortController();
    abortRef.current = abort;
    let cancelled = false;

    (async () => {
      let resolvedDescription: string | null = null;
      try {
        const convs = await api.listConversations(projectId);
        if (cancelled) return;
        if (convs.length > 0) {
          const conv = convs[0];
          const msgs = await api.listMessages(projectId, conv.id);
          if (cancelled) return;
          const hasAssistant = msgs.some((m) => m.role === "assistant");
          const lastUser = [...msgs].reverse().find((m) => m.role === "user");
          if (!hasAssistant && lastUser) {
            resolvedDescription = lastUser.content;
          }
        }
      } catch {
        // fall through
      }

      if (!resolvedDescription || cancelled) return;

      const now = Date.now();
      setDescription(resolvedDescription);
      setActive(true);
      setStartedAt(now);
      setLastEventAt(now);
      serverProgressRef.current = 0;
      setSteps([
        {
          id: "init-0",
          type: "phase",
          icon: "brain",
          message: "Analyzing your idea…",
          progress: 0,
          timestamp: now,
        },
        {
          id: "init-1",
          type: "thinking",
          icon: "eye",
          message: "Detecting app type, features, and complexity",
          timestamp: now + 1,
        },
      ]);

      // Synthetic progress ticker: slowly advances the bar between real
      // server events so the UI never looks frozen. Caps at the midpoint
      // between current and the next expected server value.
      tickerRef.current = setInterval(() => {
        setProgress((prev) => {
          const server = serverProgressRef.current;
          if (prev >= 99) return prev;
          // If server has sent a value, smoothly approach it
          if (server > prev) {
            return Math.min(server, prev + Math.ceil((server - prev) / 3));
          }
          // Otherwise creep forward slowly (max 1% every 2s, never past next milestone)
          const nextMilestone = server < 15 ? 12 : server < 40 ? 38 : server < 70 ? 68 : 95;
          if (prev < nextMilestone) {
            return prev + 1;
          }
          return prev;
        });
      }, 2000);

      const onEvent = (event: OneShotEvent) => {
        setLastEventAt(Date.now());

        // Update progress from BOTH progress events AND thinking events
        const serverPct =
          (event.type === "progress" && event.percent != null)
            ? event.percent
            : (typeof event.progress === "number" && event.progress > 0)
              ? event.progress
              : null;

        if (serverPct != null && serverPct > serverProgressRef.current) {
          serverProgressRef.current = serverPct;
          setProgress((prev) => Math.max(prev, serverPct));
        }

        const step = eventToStep(event, eventIndexRef.current++);
        if (step) setSteps((prev) => [...prev, step]);

        if (event.type === "complete") {
          serverProgressRef.current = 100;
          setProgress(100);
          setIsComplete(true);
          stopTicker();
          if (typeof event.taste_score === "number") setTasteScore(event.taste_score);
          // `app_url` is emitted by the backend's Complete event after the
          // auto-start port-poll; use it as the canonical preview URL.
          if (typeof event.app_url === "string" && event.app_url.length > 0) {
            setAppUrl(event.app_url);
          }
          queryClient.invalidateQueries({ queryKey: ["projects", projectId] });
        }
        if (event.type === "error" && event.fatal) {
          setIsError(true);
          stopTicker();
        }
      };

      try {
        await streamOneshotForProject(projectId, resolvedDescription, onEvent, abort.signal);
      } catch (e) {
        if (cancelled || abort.signal.aborted) return;
        setIsError(true);
        stopTicker();
        setSteps((prev) => [
          ...prev,
          {
            id: `stream-error-${Date.now()}`,
            type: "error",
            icon: "shield",
            message: e instanceof Error ? e.message : "Generation failed",
            fatal: true,
            timestamp: Date.now(),
          },
        ]);
      }
    })();

    return () => {
      cancelled = true;
      abort.abort();
      stopTicker();
    };
  }, [projectId, queryClient, stopTicker]);

  const abort = useCallback(() => {
    if (abortRef.current) {
      abortRef.current.abort();
      abortRef.current = null;
    }
    stopTicker();
    setAborted(true);
    setActive(false);
    // Don't flip isError on a user-initiated stop — callers distinguish
    // "aborted" from "errored" and some UI paths treat any error as fatal.
  }, [stopTicker]);

  return {
    active,
    isComplete,
    isError,
    progress,
    steps,
    description,
    completedProjectId: isComplete ? projectId : null,
    tasteScore,
    appUrl,
    startedAt,
    lastEventAt,
    abort,
    aborted,
  };
}
