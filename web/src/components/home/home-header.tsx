"use client";

import Link from "next/link";
import { usePathname } from "next/navigation";
import { useRef, useEffect, useState, useCallback } from "react";
import {
  ChevronDown, Plus, FolderOpen, Trash2,
  Cpu, BarChart2, Settings,
} from "lucide-react";
import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { ThemeToggle } from "@/components/theme-toggle";
import { NexusLogoMini } from "@/components/brand/nexus-logo";
import type { Project } from "@/lib/api";

const PHASE_LABELS: Record<number, string> = {
  1: "Exploring",
  2: "Structuring",
  3: "Building",
  4: "Published",
};

interface HomeHeaderProps {
  projects: Project[];
  onDeleteProject: (id: string) => void;
  onNewProject: () => void;
}

export function HomeHeader({ projects, onDeleteProject, onNewProject }: HomeHeaderProps) {
  const pathname = usePathname();
  const [projectsOpen, setProjectsOpen] = useState(false);
  const dropdownRef = useRef<HTMLDivElement>(null);

  // Close on outside click
  useEffect(() => {
    if (!projectsOpen) return;
    function handleClick(e: MouseEvent) {
      if (dropdownRef.current && !dropdownRef.current.contains(e.target as Node)) {
        setProjectsOpen(false);
      }
    }
    document.addEventListener("mousedown", handleClick);
    return () => document.removeEventListener("mousedown", handleClick);
  }, [projectsOpen]);

  // Close on Escape
  const handleKeyDown = useCallback((e: KeyboardEvent) => {
    if (e.key === "Escape" && projectsOpen) {
      setProjectsOpen(false);
    }
  }, [projectsOpen]);

  useEffect(() => {
    document.addEventListener("keydown", handleKeyDown);
    return () => document.removeEventListener("keydown", handleKeyDown);
  }, [handleKeyDown]);

  function navLinkClass(href: string) {
    const isActive = pathname === href;
    return cn(
      "flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-sm transition-colors",
      "focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-glow-cyan/40",
      isActive
        ? "bg-glow-cyan/[0.08] text-glow-cyan"
        : "text-slate-400 hover:text-slate-200 hover:bg-white/[0.05]",
    );
  }

  return (
    <header className="flex items-center justify-between px-6 py-3 border-b border-white/[0.06] flex-shrink-0">
      {/* Logo */}
      <Link href="/" className="flex items-center gap-3 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-glow-cyan/40 rounded-lg px-1 -ml-1">
        <NexusLogoMini size={24} animated={false} />
        <span className="font-semibold text-glow-cyan tracking-wide">NEXUS</span>
        <Badge variant="outline" className="text-[10px]">BETA</Badge>
      </Link>

      {/* Projects dropdown */}
      <div className="relative" ref={dropdownRef}>
        <button
          onClick={() => setProjectsOpen(!projectsOpen)}
          aria-expanded={projectsOpen}
          aria-haspopup="listbox"
          className={cn(
            "flex items-center gap-2 px-3 py-1.5 rounded-lg text-sm transition-colors",
            "focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-glow-cyan/40",
            "text-slate-400 hover:text-slate-200 hover:bg-white/[0.05]",
            projectsOpen && "bg-white/[0.05] text-slate-200",
          )}
        >
          <FolderOpen className="w-4 h-4" />
          <span>Projects</span>
          {projects.length > 0 && (
            <Badge variant="secondary" className="text-[10px] ml-1">{projects.length}</Badge>
          )}
          <ChevronDown className={cn("w-3.5 h-3.5 transition-transform duration-200", projectsOpen && "rotate-180")} />
        </button>

        {projectsOpen && (
          <div
            role="listbox"
            className="absolute right-0 top-full mt-2 w-80 glass-card rounded-xl border border-white/[0.1] shadow-2xl z-50 overflow-hidden animate-fade-in"
          >
            <div className="flex items-center justify-between px-4 py-3 border-b border-white/[0.06]">
              <span className="text-sm font-medium text-slate-200">Your Projects</span>
              <Button
                variant="ghost"
                size="sm"
                className="text-xs h-7"
                onClick={() => { setProjectsOpen(false); onNewProject(); }}
              >
                <Plus className="w-3 h-3 mr-1" /> New
              </Button>
            </div>

            <div className="max-h-80 overflow-y-auto scrollbar-thin">
              {projects.length === 0 ? (
                <div className="px-4 py-8 text-center">
                  <FolderOpen className="w-8 h-8 text-slate-400/20 mx-auto mb-2" />
                  <p className="text-sm text-slate-400">No projects yet</p>
                  <p className="text-xs text-slate-400/50 mt-1">Describe an idea below to get started</p>
                </div>
              ) : (
                projects.map((p) => (
                  <Link
                    key={p.id}
                    href={`/${p.id}`}
                    role="option"
                    onClick={() => setProjectsOpen(false)}
                    className={cn(
                      "w-full flex items-center gap-3 px-4 py-3 text-left transition-colors group",
                      "hover:bg-white/[0.04] focus-visible:bg-white/[0.04] focus-visible:outline-none",
                    )}
                  >
                    <div className="w-8 h-8 rounded-lg bg-glow-cyan/[0.08] flex items-center justify-center flex-shrink-0">
                      <span className="text-glow-cyan text-xs font-bold">{p.name.charAt(0).toUpperCase()}</span>
                    </div>
                    <div className="flex-1 min-w-0">
                      <p className="text-sm font-medium text-slate-200 truncate">{p.name}</p>
                      <div className="flex items-center gap-2 mt-0.5">
                        <Badge
                          variant={p.phase >= 3 ? "success" : p.phase >= 2 ? "info" : "secondary"}
                          className="text-[9px] px-1.5 py-0"
                        >
                          {PHASE_LABELS[p.phase] || `Phase ${p.phase}`}
                        </Badge>
                        <span className="text-[10px] text-slate-400/50">
                          {new Date(p.created_at).toLocaleDateString()}
                        </span>
                      </div>
                    </div>
                    <button
                      onClick={(e) => { e.preventDefault(); e.stopPropagation(); onDeleteProject(p.id); }}
                      className={cn(
                        "opacity-0 group-hover:opacity-100 p-1.5 rounded-md transition-all",
                        "hover:bg-red-500/10 text-slate-400 hover:text-red-400",
                        "focus-visible:outline-none focus-visible:opacity-100 focus-visible:ring-1 focus-visible:ring-red-400/40",
                      )}
                      aria-label={`Delete ${p.name}`}
                    >
                      <Trash2 className="w-3.5 h-3.5" />
                    </button>
                  </Link>
                ))
              )}
            </div>
          </div>
        )}
      </div>

      {/* Navigation */}
      <nav className="flex items-center gap-1" aria-label="Global navigation">
        <Link href="/agents" className={navLinkClass("/agents")}>
          <Cpu className="w-4 h-4" />
          <span className="hidden sm:inline">Agents</span>
        </Link>
        <Link href="/admin" className={navLinkClass("/admin")}>
          <BarChart2 className="w-4 h-4" />
          <span className="hidden sm:inline">Admin</span>
        </Link>
        <ThemeToggle />
        <Link href="/settings" className={navLinkClass("/settings")}>
          <Settings className="w-4 h-4" />
          <span className="hidden sm:inline">Settings</span>
        </Link>
      </nav>
    </header>
  );
}
