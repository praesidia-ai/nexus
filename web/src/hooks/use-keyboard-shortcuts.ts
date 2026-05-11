"use client";

import { useEffect, useCallback } from "react";

/**
 * Global keyboard shortcuts.
 *
 * Registers:
 *  - Cmd+K  — open command palette
 *  - Cmd+Shift+G — new generation (navigate)
 *  - Cmd+B  — toggle sidebar
 *  - Cmd+1..4 — tab shortcuts (dispatches custom events)
 *
 * All shortcuts are suppressed when an input/textarea is focused
 * (except Cmd+K which always fires).
 */

interface ShortcutHandlers {
  onTogglePalette: () => void;
  onToggleSidebar: () => void;
  onNewGeneration?: () => void;
  onTab?: (index: number) => void;
}

export function useKeyboardShortcuts(handlers: ShortcutHandlers) {
  const handleKeyDown = useCallback(
    (e: KeyboardEvent) => {
      const meta = e.metaKey || e.ctrlKey;
      const tag = (e.target as HTMLElement).tagName;
      const isInput = ["INPUT", "TEXTAREA", "SELECT"].includes(tag);

      // Cmd+K — always fires (even in inputs)
      if (meta && e.key === "k") {
        e.preventDefault();
        handlers.onTogglePalette();
        return;
      }

      // Skip remaining shortcuts when inside an input
      if (isInput) return;

      // Cmd+B — toggle sidebar
      if (meta && e.key === "b") {
        e.preventDefault();
        handlers.onToggleSidebar();
        return;
      }

      // Cmd+Shift+G — new generation
      if (meta && e.shiftKey && e.key === "G") {
        e.preventDefault();
        handlers.onNewGeneration?.();
        return;
      }

      // Cmd+1..4 — tab shortcuts
      if (meta && !e.shiftKey && e.key >= "1" && e.key <= "4") {
        e.preventDefault();
        handlers.onTab?.(parseInt(e.key, 10));
        return;
      }
    },
    [handlers],
  );

  useEffect(() => {
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [handleKeyDown]);
}
