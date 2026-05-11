"use client";

import { useQuery } from "@tanstack/react-query";
import { api } from "@/lib/api";

export interface LlmKeyStatus {
  /** True when any non-ollama provider has a configured key. */
  hasAnyKey: boolean;
  /** Provider ids with `configured: true` (excluding ollama). */
  configuredProviders: string[];
  /** True while the underlying query is still loading. */
  isLoading: boolean;
  /** True when Ollama is reachable locally — read from /health/detailed. */
  ollamaAvailable: boolean;
}

// ─── Narrow helpers for the loosely-typed /health/detailed payload ──────────

interface DetailedHealthPayload {
  subsystems?: {
    llm_providers?: {
      providers?: Record<string, unknown>;
      ollama?: unknown;
    };
    ollama?: unknown;
  };
  ollama?: unknown;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function readOllamaAvailability(payload: unknown): boolean {
  if (!isRecord(payload)) return false;

  // The shape is intentionally resilient: we check a few sensible locations so
  // we stay compatible with small backend refactors without needing a release.
  const typed = payload as DetailedHealthPayload;

  const direct = typed.ollama;
  if (typeof direct === "boolean") return direct;
  if (isRecord(direct)) {
    const status = direct["status"];
    const available = direct["available"];
    if (typeof available === "boolean") return available;
    if (typeof status === "string") return status === "ok" || status === "available";
  }

  const subsystems = typed.subsystems;
  if (isRecord(subsystems)) {
    const nestedOllama = subsystems["ollama"];
    if (typeof nestedOllama === "boolean") return nestedOllama;
    if (isRecord(nestedOllama)) {
      const available = nestedOllama["available"];
      const status = nestedOllama["status"];
      if (typeof available === "boolean") return available;
      if (typeof status === "string") return status === "ok" || status === "available";
    }

    const llm = subsystems["llm_providers"];
    if (isRecord(llm)) {
      const providers = llm["providers"];
      if (isRecord(providers)) {
        const ollama = providers["ollama"];
        if (typeof ollama === "boolean") return ollama;
      }
      const ollamaEntry = llm["ollama"];
      if (typeof ollamaEntry === "boolean") return ollamaEntry;
    }
  }

  return false;
}

/**
 * Reports whether the user has *any* LLM credentials configured, so the UI
 * can decide whether to surface the first-run onboarding gate.
 *
 * Uses React Query with a 30s stale time — the Settings page can invalidate
 * the `llm-key-status` query key to refresh immediately after a key change.
 */
export function useLlmKeyStatus(): LlmKeyStatus {
  const apiKeysQuery = useQuery({
    queryKey: ["llm-key-status", "api-keys"],
    queryFn: () => api.listApiKeys(),
    staleTime: 30_000,
  });

  const healthQuery = useQuery({
    queryKey: ["llm-key-status", "health"],
    queryFn: () => api.getHealth(),
    staleTime: 30_000,
  });

  const configuredProviders = (apiKeysQuery.data?.providers ?? [])
    .filter((p) => p.configured && p.provider !== "ollama")
    .map((p) => p.provider);

  const ollamaAvailable = healthQuery.data
    ? readOllamaAvailability(healthQuery.data)
    : false;

  return {
    hasAnyKey: configuredProviders.length > 0,
    configuredProviders,
    isLoading: apiKeysQuery.isLoading || healthQuery.isLoading,
    ollamaAvailable,
  };
}
