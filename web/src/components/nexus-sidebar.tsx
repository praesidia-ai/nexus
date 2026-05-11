"use client";

import Link from "next/link";
import { usePathname } from "next/navigation";
import { useState, useEffect, useCallback } from "react";
import {
  Search,
  Settings,
  ChevronDown,
  Circle,
  PanelLeftClose,
  PanelLeft,
  Plus,
} from "lucide-react";
import { cn } from "@/lib/utils";
import { api, type AgentDefinition } from "@/lib/api";
import { NAV_GROUPS, allNavItems } from "@/lib/navigation";
import { CostIndicator } from "@/components/cost-indicator";
import { NexusLogoMini } from "@/components/brand/nexus-logo";
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from "@/components/ui/tooltip";

interface Props {
  projectId: string;
  projectName: string;
}

export function NexusSidebar({ projectId, projectName }: Props) {
  const pathname = usePathname();

  const [collapsed, setCollapsed] = useState(false);
  const [agents, setAgents] = useState<AgentDefinition[]>([]);
  const [allProjects, setAllProjects] = useState<
    { id: string; name: string; phase: number }[]
  >([]);
  const [projectSwitcherOpen, setProjectSwitcherOpen] = useState(false);

  useEffect(() => {
    api.listAgents(projectId).then(setAgents).catch(() => {});
    api.listProjects().then(setAllProjects).catch(() => {});
  }, [projectId]);

  const handleKeyDown = useCallback((e: KeyboardEvent) => {
    if ((e.metaKey || e.ctrlKey) && e.key === "b") {
      e.preventDefault();
      setCollapsed((prev) => !prev);
    }
  }, []);

  useEffect(() => {
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [handleKeyDown]);

  const runningAgents = agents.filter((a) => a.status === "running");
  const projectsWithActivity = allProjects.filter((p) => p.phase >= 4);

  function openPalette() {
    window.dispatchEvent(
      new KeyboardEvent("keydown", { key: "k", metaKey: true }),
    );
  }

  const flatItems = allNavItems();

  // ── Collapsed sidebar ────────────────────────────────────────────────

  if (collapsed) {
    return (
      <TooltipProvider delayDuration={200}>
        <aside className="w-[52px] flex-shrink-0 bg-nexus-deep/90 backdrop-blur-xl border-r border-white/[0.06] flex flex-col h-screen items-center py-3 gap-1">
          <Link href="/" className="mb-1">
            <NexusLogoMini size={28} />
          </Link>

          <Tooltip>
            <TooltipTrigger asChild>
              <button
                onClick={() => setCollapsed(false)}
                className="w-8 h-8 rounded-lg flex items-center justify-center text-slate-400 hover:bg-white/[0.06] hover:text-slate-200 transition-colors"
                aria-label="Expand sidebar"
              >
                <PanelLeft className="w-4 h-4" />
              </button>
            </TooltipTrigger>
            <TooltipContent side="right">Expand</TooltipContent>
          </Tooltip>

          <Tooltip>
            <TooltipTrigger asChild>
              <button
                onClick={openPalette}
                className="w-8 h-8 rounded-lg flex items-center justify-center text-slate-400 hover:bg-white/[0.06] hover:text-slate-200 transition-colors"
                aria-label="Search"
              >
                <Search className="w-4 h-4" />
              </button>
            </TooltipTrigger>
            <TooltipContent side="right">Search</TooltipContent>
          </Tooltip>

          <div className="w-6 border-t border-white/[0.06] my-1" />

          {flatItems.map((item) => {
            const active = item.match(pathname, projectId);
            return (
              <Tooltip key={item.id}>
                <TooltipTrigger asChild>
                  <Link
                    href={item.href(projectId)}
                    className={cn(
                      "w-8 h-8 rounded-lg flex items-center justify-center transition-colors relative",
                      active
                        ? "bg-glow-cyan/[0.08] text-glow-cyan"
                        : "text-slate-400 hover:bg-white/[0.06] hover:text-slate-200",
                    )}
                    aria-label={item.label}
                  >
                    {active && (
                      <span className="absolute left-0 top-1/2 -translate-y-1/2 w-[3px] h-4 rounded-r-full bg-glow-cyan" />
                    )}
                    <item.icon className="w-4 h-4" />
                  </Link>
                </TooltipTrigger>
                <TooltipContent side="right">{item.label}</TooltipContent>
              </Tooltip>
            );
          })}

          <div className="flex-1" />

          {runningAgents.length > 0 && (
            <Tooltip>
              <TooltipTrigger asChild>
                <div className="flex flex-col gap-1 items-center mb-2">
                  {runningAgents.slice(0, 3).map((a) => (
                    <span
                      key={a.id}
                      className="w-2 h-2 rounded-full bg-emerald-400 animate-pulse"
                    />
                  ))}
                  {runningAgents.length > 3 && (
                    <span className="text-[9px] text-slate-500">
                      +{runningAgents.length - 3}
                    </span>
                  )}
                </div>
              </TooltipTrigger>
              <TooltipContent side="right">
                {runningAgents.length} agent
                {runningAgents.length !== 1 ? "s" : ""} running
              </TooltipContent>
            </Tooltip>
          )}

          <Tooltip>
            <TooltipTrigger asChild>
              <Link
                href="/settings"
                className={cn(
                  "w-8 h-8 rounded-lg flex items-center justify-center transition-colors",
                  pathname === "/settings"
                    ? "bg-glow-cyan/[0.08] text-glow-cyan"
                    : "text-slate-400 hover:bg-white/[0.06] hover:text-slate-200",
                )}
                aria-label="Settings"
              >
                <Settings className="w-4 h-4" />
              </Link>
            </TooltipTrigger>
            <TooltipContent side="right">Settings</TooltipContent>
          </Tooltip>
        </aside>
      </TooltipProvider>
    );
  }

  // ── Expanded sidebar ─────────────────────────────────────────────────

  return (
    <aside className="w-[220px] flex-shrink-0 bg-nexus-deep/90 backdrop-blur-xl border-r border-white/[0.06] flex flex-col h-screen overflow-hidden">
      {/* Header: Logo + collapse */}
      <div className="px-3 pt-3 pb-2 flex items-center gap-2">
        <Link href="/" className="flex items-center gap-2 flex-1 min-w-0">
          <NexusLogoMini size={24} animated={false} />
          <span className="font-semibold text-glow-cyan tracking-wide truncate">
            NEXUS
          </span>
        </Link>
        <button
          onClick={() => setCollapsed(true)}
          className="w-6 h-6 rounded flex items-center justify-center text-slate-500 hover:text-slate-400 hover:bg-white/[0.06] transition-colors"
          aria-label="Collapse sidebar"
        >
          <PanelLeftClose className="w-3.5 h-3.5" />
        </button>
      </div>

      {/* Search trigger */}
      <div className="px-3 pb-2">
        <button
          onClick={openPalette}
          className="w-full flex items-center gap-2 px-2.5 py-1.5 rounded-lg border border-white/[0.06] bg-white/[0.02] text-[11px] text-slate-500 hover:text-slate-400 hover:bg-white/[0.04] transition-colors"
        >
          <Search className="w-3 h-3 flex-shrink-0" />
          <span className="flex-1 text-left">Search...</span>
          <kbd className="px-1.5 py-0.5 rounded bg-white/[0.06] font-mono text-[9px] select-none">
            {"\u2318"}K
          </kbd>
        </button>
      </div>

      {/* Project switcher */}
      <div className="px-3 pb-2 border-b border-white/[0.06] relative">
        <button
          onClick={() => setProjectSwitcherOpen(!projectSwitcherOpen)}
          className="w-full flex items-center gap-2 px-2 py-2 rounded-lg hover:bg-white/[0.05] transition-colors"
        >
          <div className="w-6 h-6 rounded-md bg-glow-cyan/[0.08] flex items-center justify-center flex-shrink-0">
            <span className="text-glow-cyan text-[10px] font-bold">
              {projectName.charAt(0).toUpperCase()}
            </span>
          </div>
          <span className="font-medium text-[13px] text-slate-200 truncate flex-1 text-left">
            {projectName}
          </span>
          <ChevronDown
            className={cn(
              "w-3 h-3 text-slate-400 transition-transform flex-shrink-0",
              projectSwitcherOpen && "rotate-180",
            )}
          />
        </button>

        {projectSwitcherOpen && (
          <div className="absolute left-2 right-2 top-full mt-1 glass-card rounded-lg border border-white/[0.1] shadow-2xl z-50 overflow-hidden">
            <div className="max-h-60 overflow-y-auto scrollbar-thin py-1">
              {allProjects.map((p) => (
                <Link
                  key={p.id}
                  href={`/${p.id}`}
                  onClick={() => setProjectSwitcherOpen(false)}
                  className={cn(
                    "flex items-center gap-2 px-3 py-2 text-[12px] transition-colors",
                    p.id === projectId
                      ? "bg-glow-cyan/[0.08] text-glow-cyan"
                      : "text-slate-400 hover:bg-white/[0.05] hover:text-slate-200",
                  )}
                >
                  <div className="w-5 h-5 rounded bg-glow-cyan/[0.08] flex items-center justify-center flex-shrink-0">
                    <span className="text-glow-cyan text-[9px] font-bold">
                      {p.name.charAt(0).toUpperCase()}
                    </span>
                  </div>
                  <span className="truncate flex-1">{p.name}</span>
                  {projectsWithActivity.some((pa) => pa.id === p.id) && (
                    <span className="w-1.5 h-1.5 rounded-full bg-emerald-400 flex-shrink-0" />
                  )}
                  {p.id === projectId && (
                    <Circle className="w-1.5 h-1.5 fill-glow-cyan text-glow-cyan flex-shrink-0" />
                  )}
                </Link>
              ))}
            </div>
            <div className="border-t border-white/[0.06] p-1">
              <Link
                href="/"
                className="flex items-center gap-2 px-3 py-2 text-[12px] text-slate-400 hover:text-slate-200 hover:bg-white/[0.05] rounded transition-colors"
              >
                <Plus className="w-3 h-3" />
                New Project
              </Link>
            </div>
          </div>
        )}
      </div>

      {/* Grouped navigation */}
      <nav className="flex-1 overflow-y-auto scrollbar-thin py-2">
        {NAV_GROUPS.map((group, gi) => (
          <div key={group.id} className={cn(gi > 0 && "mt-3")}>
            <span className="block px-4 pb-1 text-[10px] uppercase tracking-wider text-slate-600 font-medium select-none">
              {group.label}
            </span>
            {group.items.map((item) => {
              const active = item.match(pathname, projectId);
              return (
                <Link
                  key={item.id}
                  href={item.href(projectId)}
                  className={cn(
                    "flex items-center gap-2 mx-2 px-2 py-1.5 rounded-md text-[12px] transition-colors relative",
                    active
                      ? "bg-glow-cyan/[0.08] text-glow-cyan font-medium"
                      : "text-slate-500 hover:bg-white/[0.05] hover:text-slate-400",
                  )}
                >
                  {active && (
                    <span className="absolute left-0 top-1/2 -translate-y-1/2 w-[3px] h-4 rounded-r-full bg-glow-cyan" />
                  )}
                  <item.icon className="w-3.5 h-3.5 flex-shrink-0" />
                  <span>{item.label}</span>
                  {item.id === "agents" && runningAgents.length > 0 && (
                    <span className="ml-auto flex items-center gap-1 text-[10px] text-emerald-400">
                      <span className="w-1.5 h-1.5 rounded-full bg-emerald-400 animate-pulse" />
                      {runningAgents.length}
                    </span>
                  )}
                </Link>
              );
            })}
          </div>
        ))}
      </nav>

      {/* Cost indicator */}
      <div className="mx-3 mb-2 px-1">
        <CostIndicator />
      </div>

      {/* Settings */}
      <div className="px-2 pb-1">
        <Link
          href="/settings"
          className={cn(
            "flex items-center gap-2 px-2 py-1.5 rounded-md text-[13px] transition-all duration-150 relative",
            pathname === "/settings"
              ? "bg-glow-cyan/[0.08] text-glow-cyan font-medium"
              : "text-slate-400 hover:bg-white/[0.05] hover:text-slate-200",
          )}
        >
          {pathname === "/settings" && (
            <span className="absolute left-0 top-1/2 -translate-y-1/2 w-[3px] h-4 rounded-r-full bg-glow-cyan" />
          )}
          <Settings className="w-3.5 h-3.5" />
          <span>Settings</span>
          <kbd className="ml-auto px-1 py-0.5 rounded bg-white/[0.04] text-[9px] font-mono text-slate-600 select-none">
            {"\u2318"},
          </kbd>
        </Link>
      </div>

      {/* User info */}
      {(() => {
        const name = process.env.NEXT_PUBLIC_USER_NAME || "";
        const accountType = process.env.NEXT_PUBLIC_ACCOUNT_TYPE || "";
        if (!name && !accountType) return null;
        const initials = name
          .split(" ")
          .filter(Boolean)
          .map((w) => w[0].toUpperCase())
          .slice(0, 2)
          .join("");
        return (
          <div className="px-4 py-3 border-t border-white/[0.06]">
            <div className="flex items-center gap-2">
              <div className="w-7 h-7 rounded-full bg-gradient-to-br from-glow-cyan to-glow-blue flex items-center justify-center flex-shrink-0 shadow-glow-sm">
                <span className="text-white text-xs font-semibold">
                  {initials || "?"}
                </span>
              </div>
              <div className="min-w-0">
                {name && (
                  <p className="text-[13px] font-medium text-slate-200 truncate">
                    {name}
                  </p>
                )}
                {accountType && (
                  <p className="text-[11px] text-slate-400">{accountType}</p>
                )}
              </div>
            </div>
          </div>
        );
      })()}
    </aside>
  );
}
