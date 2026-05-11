"use client";

import { useState, useCallback, useRef } from "react";
import { type OneShotEvent, streamOneshot } from "@/lib/api";

export interface GenerationEvent {
  id: string;
  type: string;
  timestamp: Date;
  data: OneShotEvent;
}

export interface GenerationState {
  phase: string;
  progress: number;
  events: GenerationEvent[];
  files: string[];
  tasteScore: number | null;
  isComplete: boolean;
  isRunning: boolean;
  error: string | null;
  projectId: string | null;
  durationMs: number | null;
  intent: {
    appType?: string;
    complexity?: string;
    domain?: string;
    needsAuth?: boolean;
    needsDatabase?: boolean;
  } | null;
  decisions: {
    frontend?: string;
    database?: string;
    auth?: string;
    learningOverrides?: string[];
  } | null;
  costEstimate: {
    totalEstimatedMs?: number;
  } | null;
}

const INITIAL_STATE: GenerationState = {
  phase: "idle",
  progress: 0,
  events: [],
  files: [],
  tasteScore: null,
  isComplete: false,
  isRunning: false,
  error: null,
  projectId: null,
  durationMs: null,
  intent: null,
  decisions: null,
  costEstimate: null,
};

let eventCounter = 0;

export function useGeneration(): {
  state: GenerationState;
  start: (description: string, options?: { autoRedesign?: boolean; tasteThreshold?: number }) => Promise<void>;
  reset: () => void;
} {
  const [state, setState] = useState<GenerationState>(INITIAL_STATE);
  const abortRef = useRef<AbortController | null>(null);

  const reset = useCallback(() => {
    if (abortRef.current) {
      abortRef.current.abort();
      abortRef.current = null;
    }
    setState(INITIAL_STATE);
  }, []);

  const start = useCallback(
    async (
      description: string,
      options?: { autoRedesign?: boolean; tasteThreshold?: number }
    ) => {
      // Reset and start
      if (abortRef.current) abortRef.current.abort();
      const controller = new AbortController();
      abortRef.current = controller;

      setState({
        ...INITIAL_STATE,
        phase: "starting",
        isRunning: true,
      });

      try {
        await streamOneshot(
          description,
          (event: OneShotEvent) => {
            const entry: GenerationEvent = {
              id: String(++eventCounter),
              type: event.type,
              timestamp: new Date(),
              data: event,
            };

            setState((prev) => {
              const next = { ...prev };
              next.events = [...prev.events, entry];

              // Phase tracking
              if (event.type === "phase") {
                next.phase = event.phase ?? prev.phase;
              }

              // Progress
              if (event.type === "progress" && event.percent !== undefined) {
                next.progress = event.percent;
              }

              // Intent
              if (event.type === "intent") {
                next.intent = {
                  appType: event.app_type,
                  complexity: event.complexity,
                  domain: event.domain,
                  needsAuth: event.needs_auth,
                  needsDatabase: event.needs_database,
                };
              }

              // Decisions
              if (event.type === "decisions") {
                next.decisions = {
                  frontend: event.frontend,
                  database: event.database,
                  auth: event.auth,
                  learningOverrides: event.learning_overrides,
                };
              }

              // File events
              if (event.type === "file" && event.path) {
                next.files = [...prev.files, event.path];
              }

              // Taste score
              if (event.type === "taste" && event.overall !== undefined) {
                next.tasteScore = event.overall;
              }

              // Cost estimate
              if (event.type === "estimate") {
                next.costEstimate = {
                  totalEstimatedMs: event.total_estimated_ms,
                };
              }

              // Completion
              if (event.type === "complete") {
                next.isComplete = true;
                next.isRunning = false;
                next.progress = 100;
                next.phase = "complete";
                next.projectId = event.project_id ?? prev.projectId;
                next.durationMs = event.duration_ms ?? null;
                if (event.taste_score !== undefined) {
                  next.tasteScore = event.taste_score ?? null;
                }
                if (event.files_count !== undefined) {
                  // Ensure files list length is accurate
                }
              }

              // Error
              if (event.type === "error") {
                next.error = event.message ?? "Unknown error";
                if (event.fatal) {
                  next.isRunning = false;
                }
              }

              return next;
            });
          },
          controller.signal,
          {
            auto_redesign: options?.autoRedesign,
            taste_threshold: options?.tasteThreshold,
          }
        );
      } catch (err: unknown) {
        if (err instanceof Error && err.name === "AbortError") return;
        setState((prev) => ({
          ...prev,
          error: err instanceof Error ? err.message : "Generation failed",
          isRunning: false,
        }));
      }
    },
    []
  );

  return { state, start, reset };
}
