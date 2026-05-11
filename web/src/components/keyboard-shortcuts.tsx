"use client";

import { useState, useEffect, useCallback } from "react";
import { X } from "lucide-react";

interface Shortcut {
  keys: string[];
  description: string;
  section: string;
}

/// Only list bindings that are actually wired up. Ghost entries train users
/// to give up on the palette — worse than having fewer shortcuts.
const SHORTCUTS: Shortcut[] = [
  { keys: ["Ctrl", "K"], description: "Open command palette", section: "Navigation" },
  { keys: ["Ctrl", "B"], description: "Toggle sidebar", section: "Navigation" },
  { keys: ["?"], description: "Show keyboard shortcuts", section: "Navigation" },
  { keys: ["Esc"], description: "Close dialog / deselect", section: "Navigation" },
  { keys: ["Ctrl", "."], description: "Stop generation", section: "Generation" },
  { keys: ["Enter"], description: "Send message / submit", section: "General" },
  { keys: ["Shift", "Enter"], description: "New line in text inputs", section: "General" },
  { keys: ["Arrow Up/Down"], description: "Navigate lists", section: "General" },
];

export function KeyboardShortcuts() {
  const [open, setOpen] = useState(false);

  const handleKeyDown = useCallback((e: KeyboardEvent) => {
    const inInput = ["INPUT", "TEXTAREA", "SELECT"].includes(
      (e.target as HTMLElement).tagName,
    );
    // Only fire "?" when no input/textarea is focused
    if (e.key === "?" && !inInput) {
      e.preventDefault();
      setOpen((prev) => !prev);
    }
    if (e.key === "Escape") {
      setOpen(false);
    }
    // Cmd/Ctrl+. => broadcast a "nexus:stop-generation" event that any
    // active generation view can listen for.
    if ((e.metaKey || e.ctrlKey) && e.key === ".") {
      e.preventDefault();
      window.dispatchEvent(new CustomEvent("nexus:stop-generation"));
    }
  }, []);

  useEffect(() => {
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [handleKeyDown]);

  if (!open) return null;

  const sections = [...new Set(SHORTCUTS.map((s) => s.section))];
  const isMac =
    typeof navigator !== "undefined" && /Mac/.test(navigator.userAgent);

  function renderKey(key: string) {
    const mapped = isMac ? key.replace("Ctrl", "\u2318") : key;
    return (
      <kbd
        key={key}
        className="px-1.5 py-0.5 rounded bg-white/[0.08] border border-white/[0.1] text-[11px] font-mono text-slate-400"
      >
        {mapped}
      </kbd>
    );
  }

  return (
    <div className="fixed inset-0 z-[200]">
      <div
        className="absolute inset-0 bg-black/60 backdrop-blur-sm"
        onClick={() => setOpen(false)}
      />
      <div className="absolute top-[15%] left-1/2 -translate-x-1/2 w-full max-w-md">
        <div className="glass-card border border-white/[0.12] rounded-2xl shadow-2xl overflow-hidden">
          <div className="flex items-center justify-between px-5 py-4 border-b border-white/[0.08]">
            <h2 className="text-sm font-semibold text-slate-200">
              Keyboard Shortcuts
            </h2>
            <button
              onClick={() => setOpen(false)}
              className="text-slate-400 hover:text-slate-200 transition-colors"
              aria-label="Close shortcuts overlay"
            >
              <X className="w-4 h-4" />
            </button>
          </div>

          <div className="px-5 py-4 space-y-5">
            {sections.map((section) => (
              <div key={section}>
                <h3 className="text-[10px] uppercase tracking-wider text-slate-400/50 font-medium mb-2">
                  {section}
                </h3>
                <div className="space-y-2">
                  {SHORTCUTS.filter((s) => s.section === section).map(
                    (shortcut) => (
                      <div
                        key={shortcut.description}
                        className="flex items-center justify-between"
                      >
                        <span className="text-sm text-slate-400">
                          {shortcut.description}
                        </span>
                        <div className="flex items-center gap-1">
                          {shortcut.keys.map((key) => renderKey(key))}
                        </div>
                      </div>
                    ),
                  )}
                </div>
              </div>
            ))}
          </div>

          <div className="border-t border-white/[0.08] px-5 py-3">
            <p className="text-[11px] text-slate-400/40 text-center">
              Press{" "}
              <kbd className="px-1 py-0.5 rounded bg-white/[0.06] font-mono text-[10px]">
                ?
              </kbd>{" "}
              to toggle this overlay
            </p>
          </div>
        </div>
      </div>
    </div>
  );
}
