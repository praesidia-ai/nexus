"use client";

import { useState, useEffect, useRef, useCallback, useMemo } from "react";
import { useRouter, usePathname } from "next/navigation";
import { Search, Clock } from "lucide-react";
import { cn } from "@/lib/utils";
import {
  buildCommands,
  fuzzySearch,
  getRecentCommandIds,
  addRecentCommand,
  sortedCategories,
  type CommandAction,
  type CommandCategory,
} from "@/lib/commands";

// ─── Component ──────────────────────────────────────────────────────────────

export function CommandPalette() {
  const router = useRouter();
  const pathname = usePathname();
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [selectedIndex, setSelectedIndex] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLDivElement>(null);

  // Derive project context from the URL
  const projectId = pathname.match(/^\/([a-zA-Z0-9_-]+)\//)?.[1] ?? null;

  // Build the full command list once per projectId change
  const allCommands = useMemo(
    () => buildCommands({ projectId }),
    [projectId],
  );

  // Inject recent commands at the top when there is no search query
  const recentIds = useMemo(() => getRecentCommandIds(), [open]); // eslint-disable-line react-hooks/exhaustive-deps

  const commandsWithRecent = useMemo(() => {
    if (query.trim()) return allCommands;

    const recentCmds: CommandAction[] = [];
    for (const id of recentIds) {
      const found = allCommands.find((c) => c.id === id);
      if (found) {
        recentCmds.push({ ...found, category: "Recent" as CommandCategory });
      }
    }
    return [...recentCmds, ...allCommands];
  }, [allCommands, recentIds, query]);

  // Fuzzy-filtered results
  const filtered = useMemo(
    () => (query.trim() ? fuzzySearch(allCommands, query) : commandsWithRecent),
    [allCommands, commandsWithRecent, query],
  );

  // Group by category, preserving category order
  const grouped = useMemo(() => {
    const map = new Map<CommandCategory, CommandAction[]>();
    for (const cmd of filtered) {
      const list = map.get(cmd.category) ?? [];
      list.push(cmd);
      map.set(cmd.category, list);
    }
    const categories = sortedCategories([...map.keys()]);
    return categories.map((cat) => ({ category: cat, items: map.get(cat)! }));
  }, [filtered]);

  // Flat list for keyboard navigation
  const flatItems = useMemo(
    () => grouped.flatMap((g) => g.items),
    [grouped],
  );

  // Reset selection when query changes
  useEffect(() => {
    setSelectedIndex(0);
  }, [query]);

  // Scroll selected item into view
  useEffect(() => {
    if (!listRef.current) return;
    const el = listRef.current.querySelector(`[data-index="${selectedIndex}"]`);
    if (el) {
      el.scrollIntoView({ block: "nearest" });
    }
  }, [selectedIndex]);

  // Global Cmd+K listener
  const handleGlobalKey = useCallback((e: KeyboardEvent) => {
    if ((e.metaKey || e.ctrlKey) && e.key === "k") {
      e.preventDefault();
      setOpen((prev) => !prev);
      setQuery("");
    }
    if (e.key === "Escape") {
      setOpen(false);
    }
  }, []);

  useEffect(() => {
    window.addEventListener("keydown", handleGlobalKey);
    return () => window.removeEventListener("keydown", handleGlobalKey);
  }, [handleGlobalKey]);

  // Auto-focus input on open
  useEffect(() => {
    if (open) {
      requestAnimationFrame(() => inputRef.current?.focus());
    }
  }, [open]);

  // ── Execute a command ──────────────────────────────────────────────────

  function execute(cmd: CommandAction) {
    addRecentCommand(cmd.id);
    if (cmd.href) {
      router.push(cmd.href);
    }
    setOpen(false);
    setQuery("");
  }

  // ── Input keyboard navigation ─────────────────────────────────────────

  function handleInputKeyDown(e: React.KeyboardEvent) {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setSelectedIndex((i) => Math.min(i + 1, flatItems.length - 1));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setSelectedIndex((i) => Math.max(i - 1, 0));
    } else if (e.key === "Enter") {
      e.preventDefault();
      if (flatItems[selectedIndex]) {
        execute(flatItems[selectedIndex]);
      }
    }
  }

  if (!open) return null;

  // ── Render ─────────────────────────────────────────────────────────────

  let flatIdx = 0;

  return (
    <div className="fixed inset-0 z-[200]" role="dialog" aria-modal="true" aria-label="Command palette">
      {/* Backdrop */}
      <div
        className="absolute inset-0 bg-black/60 backdrop-blur-sm"
        onClick={() => setOpen(false)}
      />

      {/* Palette */}
      <div className="absolute top-[18%] left-1/2 -translate-x-1/2 w-full max-w-[520px] px-4">
        <div data-command-palette className="glass-card border border-white/[0.12] rounded-2xl shadow-2xl overflow-hidden">
          {/* Search input */}
          <div className="flex items-center gap-3 px-4 py-3 border-b border-white/[0.08]">
            <Search className="w-4 h-4 text-slate-400 flex-shrink-0" />
            <input
              ref={inputRef}
              type="text"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              onKeyDown={handleInputKeyDown}
              placeholder="Type a command or search..."
              className="flex-1 bg-transparent text-sm text-slate-200 placeholder:text-slate-400/50 focus:outline-none"
              aria-label="Search commands"
            />
            <kbd className="px-1.5 py-0.5 rounded bg-white/[0.06] text-[10px] font-mono text-slate-400/50 select-none">
              ESC
            </kbd>
          </div>

          {/* Command list */}
          <div
            ref={listRef}
            className="max-h-80 overflow-y-auto scrollbar-thin py-1"
          >
            {flatItems.length === 0 && (
              <div className="py-10 text-center text-sm text-slate-400">
                No matching commands
              </div>
            )}

            {grouped.map((group) => {
              const startIdx = flatIdx;
              return (
                <div key={group.category}>
                  {/* Category header */}
                  <div className="flex items-center gap-2 px-4 pt-2 pb-1">
                    {group.category === "Recent" && (
                      <Clock className="w-3 h-3 text-slate-400/40" />
                    )}
                    <span className="text-[10px] uppercase tracking-wider text-slate-400/50 font-medium">
                      {group.category}
                    </span>
                  </div>

                  {/* Commands */}
                  {group.items.map((cmd) => {
                    const idx = flatIdx++;
                    const isSelected = idx === selectedIndex;
                    return (
                      <button
                        key={`${group.category}-${cmd.id}`}
                        data-index={idx}
                        onClick={() => execute(cmd)}
                        onMouseEnter={() => setSelectedIndex(startIdx + group.items.indexOf(cmd))}
                        className={cn(
                          "w-full flex items-center gap-3 px-4 py-2 text-sm transition-colors",
                          isSelected
                            ? "bg-glow-cyan/[0.08] text-glow-cyan"
                            : "text-slate-400 hover:bg-white/[0.04]",
                        )}
                      >
                        <cmd.icon className="w-4 h-4 flex-shrink-0" />
                        <div className="flex-1 flex items-baseline gap-2 text-left min-w-0">
                          <span className="truncate">{cmd.label}</span>
                          {cmd.description && (
                            <span className="text-[11px] text-slate-400/40 truncate hidden sm:inline">
                              {cmd.description}
                            </span>
                          )}
                        </div>
                        {cmd.shortcut && (
                          <kbd className="ml-auto px-1.5 py-0.5 rounded bg-white/[0.06] text-[10px] font-mono text-slate-400/40 flex-shrink-0 select-none">
                            {cmd.shortcut}
                          </kbd>
                        )}
                      </button>
                    );
                  })}
                </div>
              );
            })}
          </div>

          {/* Footer hints */}
          <div className="border-t border-white/[0.08] px-4 py-2 flex items-center gap-4 text-[10px] text-slate-400/40">
            <span className="flex items-center gap-1">
              <kbd className="px-1 py-0.5 rounded bg-white/[0.06] font-mono">&#8593;&#8595;</kbd>
              navigate
            </span>
            <span className="flex items-center gap-1">
              <kbd className="px-1 py-0.5 rounded bg-white/[0.06] font-mono">&#9166;</kbd>
              select
            </span>
            <span className="flex items-center gap-1">
              <kbd className="px-1 py-0.5 rounded bg-white/[0.06] font-mono">esc</kbd>
              close
            </span>
            <span className="ml-auto text-slate-400/30">
              {flatItems.length} command{flatItems.length !== 1 ? "s" : ""}
            </span>
          </div>
        </div>
      </div>
    </div>
  );
}
