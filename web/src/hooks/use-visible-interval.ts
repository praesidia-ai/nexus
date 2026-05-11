"use client";

import { useEffect, useRef } from "react";

/**
 * Schedule a callback on an interval that automatically pauses while the
 * browser tab is hidden and resumes on visibility.
 *
 * This saves battery / bandwidth on tabs the user is not actively looking at
 * and is preferable to plain `setInterval` for any polling work.
 *
 * The callback is stored in a ref so consumers can pass a fresh function on
 * every render without re-scheduling the interval.
 *
 * @param fn      work to run each tick
 * @param delayMs tick period in ms; pass `null` to disable the interval
 * @param opts.runOnShow  fire `fn` once when the tab becomes visible (default: true)
 */
export function useVisibleInterval(
  fn: () => void,
  delayMs: number | null,
  opts: { runOnShow?: boolean } = {},
): void {
  const { runOnShow = true } = opts;
  const savedRef = useRef(fn);

  useEffect(() => {
    savedRef.current = fn;
  }, [fn]);

  useEffect(() => {
    if (delayMs == null || delayMs <= 0) return;

    let timer: ReturnType<typeof setInterval> | null = null;
    const start = () => {
      if (timer != null) return;
      timer = setInterval(() => savedRef.current(), delayMs);
    };
    const stop = () => {
      if (timer != null) {
        clearInterval(timer);
        timer = null;
      }
    };

    const onVisibility = () => {
      if (typeof document === "undefined") return;
      if (document.hidden) {
        stop();
      } else {
        if (runOnShow) savedRef.current();
        start();
      }
    };

    if (typeof document !== "undefined" && document.hidden) {
      // Tab starts hidden — do nothing until it becomes visible.
    } else {
      start();
    }

    if (typeof document !== "undefined") {
      document.addEventListener("visibilitychange", onVisibility);
    }
    return () => {
      stop();
      if (typeof document !== "undefined") {
        document.removeEventListener("visibilitychange", onVisibility);
      }
    };
  }, [delayMs, runOnShow]);
}
