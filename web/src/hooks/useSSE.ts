"use client";

import { useState, useEffect, useRef, useCallback } from "react";

type SSEStatus = "connecting" | "connected" | "disconnected" | "error";

interface UseSSEOptions<T> {
  enabled?: boolean;
  eventType?: string;
  onMessage?: (event: T) => void;
  onError?: (error: Event) => void;
  reconnect?: boolean;
  reconnectDelay?: number;
  maxReconnects?: number;
}

interface UseSSEReturn<T> {
  status: SSEStatus;
  lastEvent: T | null;
  close: () => void;
}

/**
 * Production-grade SSE connection hook.
 *
 * Features:
 * - Auto-reconnect with exponential backoff (1s → 2s → 4s → ... max 30s)
 * - Max reconnect attempts (default 10)
 * - Status tracking (connecting/connected/disconnected/error)
 * - Cleanup on unmount
 * - Stable callback refs to avoid re-subscriptions
 */
export function useSSE<T>(
  url: string | null,
  onEvent: (event: T) => void,
  options?: UseSSEOptions<T>,
): UseSSEReturn<T> {
  const {
    enabled = true,
    eventType,
    onError,
    reconnect = true,
    reconnectDelay = 1000,
    maxReconnects = 10,
  } = options ?? {};

  const [status, setStatus] = useState<SSEStatus>("disconnected");
  const [lastEvent, setLastEvent] = useState<T | null>(null);

  const onEventRef = useRef(onEvent);
  onEventRef.current = onEvent;

  const onErrorRef = useRef(onError);
  onErrorRef.current = onError;

  const esRef = useRef<EventSource | null>(null);
  const reconnectTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const reconnectCountRef = useRef(0);
  const closedManuallyRef = useRef(false);

  const clearReconnectTimer = useCallback(() => {
    if (reconnectTimerRef.current) {
      clearTimeout(reconnectTimerRef.current);
      reconnectTimerRef.current = null;
    }
  }, []);

  const close = useCallback(() => {
    closedManuallyRef.current = true;
    clearReconnectTimer();
    if (esRef.current) {
      esRef.current.close();
      esRef.current = null;
    }
    setStatus("disconnected");
  }, [clearReconnectTimer]);

  const connect = useCallback(() => {
    if (!url || !enabled) return;

    closedManuallyRef.current = false;
    setStatus("connecting");

    const es = new EventSource(url);
    esRef.current = es;

    es.onopen = () => {
      setStatus("connected");
      reconnectCountRef.current = 0;
    };

    const handleMessage = (e: MessageEvent) => {
      try {
        const data = JSON.parse(e.data) as T;
        setLastEvent(data);
        onEventRef.current(data);
      } catch {
        // ignore malformed JSON
      }
    };

    if (eventType) {
      es.addEventListener(eventType, handleMessage);
    } else {
      es.onmessage = handleMessage;
    }

    es.onerror = (e) => {
      es.close();
      esRef.current = null;
      onErrorRef.current?.(e);

      if (closedManuallyRef.current) return;

      if (reconnect && reconnectCountRef.current < maxReconnects) {
        const backoff = Math.min(
          reconnectDelay * Math.pow(2, reconnectCountRef.current),
          30000,
        );
        reconnectCountRef.current += 1;
        setStatus("connecting");
        reconnectTimerRef.current = setTimeout(connect, backoff);
      } else {
        setStatus("error");
      }
    };
  }, [url, enabled, eventType, reconnect, reconnectDelay, maxReconnects]);

  useEffect(() => {
    if (!url || !enabled) {
      close();
      return;
    }

    connect();

    return () => {
      closedManuallyRef.current = true;
      clearReconnectTimer();
      if (esRef.current) {
        esRef.current.close();
        esRef.current = null;
      }
    };
  }, [url, enabled, connect, close, clearReconnectTimer]);

  return { status, lastEvent, close };
}
