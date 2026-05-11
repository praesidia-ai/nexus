"use client";

import { useState } from "react";
import {
  AlertTriangle,
  AlertCircle,
  Info,
  FileCode2,
  Wrench,
  Filter,
} from "lucide-react";
import { cn } from "@/lib/utils";
import { Badge } from "@/components/ui/badge";

export interface TasteFinding {
  id: string;
  severity: "critical" | "major" | "minor";
  file: string;
  line?: number;
  description: string;
  fixSuggestion?: string;
  autoFixable: boolean;
  category?: string;
}

const SEVERITY_CONFIG = {
  critical: {
    icon: AlertCircle,
    color: "text-red-400",
    bg: "bg-red-500/10",
    border: "border-red-500/20",
    label: "Critical",
    badgeVariant: "destructive" as const,
  },
  major: {
    icon: AlertTriangle,
    color: "text-amber-400",
    bg: "bg-amber-500/10",
    border: "border-amber-500/20",
    label: "Major",
    badgeVariant: "warning" as const,
  },
  minor: {
    icon: Info,
    color: "text-sky-400",
    bg: "bg-sky-500/10",
    border: "border-sky-500/20",
    label: "Minor",
    badgeVariant: "info" as const,
  },
};

type SeverityFilter = "all" | "critical" | "major" | "minor";

interface Props {
  findings: TasteFinding[];
  onAutoFix?: (findingId: string) => void;
  className?: string;
}

export function FindingsList({ findings, onAutoFix, className }: Props) {
  const [filter, setFilter] = useState<SeverityFilter>("all");
  const [expandedId, setExpandedId] = useState<string | null>(null);

  const filtered = filter === "all" ? findings : findings.filter((f) => f.severity === filter);

  const counts = {
    critical: findings.filter((f) => f.severity === "critical").length,
    major: findings.filter((f) => f.severity === "major").length,
    minor: findings.filter((f) => f.severity === "minor").length,
  };

  // Group by severity
  const grouped = {
    critical: filtered.filter((f) => f.severity === "critical"),
    major: filtered.filter((f) => f.severity === "major"),
    minor: filtered.filter((f) => f.severity === "minor"),
  };

  if (findings.length === 0) {
    return (
      <div className={cn("text-center py-8 text-sm text-slate-400", className)}>
        No taste findings to display
      </div>
    );
  }

  return (
    <div className={cn("space-y-4", className)}>
      {/* Filter bar */}
      <div className="flex items-center gap-2">
        <Filter className="h-3.5 w-3.5 text-slate-400 shrink-0" />
        {(["all", "critical", "major", "minor"] as SeverityFilter[]).map((f) => (
          <button
            key={f}
            onClick={() => setFilter(f)}
            className={cn(
              "px-2.5 py-1 rounded-full text-[11px] font-medium transition-colors border",
              filter === f
                ? "bg-glow-cyan/[0.08] text-glow-cyan border-glow-cyan/30"
                : "text-slate-400 border-white/[0.06] hover:border-white/[0.12]"
            )}
          >
            {f === "all" ? `All (${findings.length})` : `${f.charAt(0).toUpperCase() + f.slice(1)} (${counts[f]})`}
          </button>
        ))}
      </div>

      {/* Findings list */}
      <div className="space-y-1 max-h-[480px] overflow-y-auto scrollbar-thin">
        {(["critical", "major", "minor"] as const).map((severity) => {
          const items = grouped[severity];
          if (items.length === 0) return null;
          const config = SEVERITY_CONFIG[severity];

          return (
            <div key={severity}>
              {filter === "all" && (
                <div className="flex items-center gap-2 px-2 py-1.5 mt-2 first:mt-0">
                  <config.icon className={cn("h-3.5 w-3.5", config.color)} />
                  <span className={cn("text-[11px] font-semibold uppercase tracking-wider", config.color)}>
                    {config.label} ({items.length})
                  </span>
                </div>
              )}

              {items.map((finding) => {
                const fConfig = SEVERITY_CONFIG[finding.severity];
                const isExpanded = expandedId === finding.id;

                return (
                  <div
                    key={finding.id}
                    className={cn(
                      "rounded-lg border transition-colors",
                      fConfig.border,
                      isExpanded ? fConfig.bg : "bg-transparent hover:bg-white/[0.02]"
                    )}
                  >
                    <button
                      onClick={() => setExpandedId(isExpanded ? null : finding.id)}
                      className="w-full flex items-start gap-2.5 px-3 py-2 text-left"
                    >
                      <fConfig.icon className={cn("h-3.5 w-3.5 mt-0.5 shrink-0", fConfig.color)} />
                      <div className="flex-1 min-w-0">
                        <p className="text-[12px] text-slate-200 leading-snug">
                          {finding.description}
                        </p>
                        <div className="flex items-center gap-2 mt-1">
                          <span className="text-[10px] text-slate-400 flex items-center gap-1">
                            <FileCode2 className="h-2.5 w-2.5" />
                            {finding.file}
                            {finding.line != null && `:${finding.line}`}
                          </span>
                          {finding.autoFixable && (
                            <Badge variant="success" className="text-[9px] px-1.5 py-0">
                              auto-fixable
                            </Badge>
                          )}
                        </div>
                      </div>
                    </button>

                    {isExpanded && finding.fixSuggestion && (
                      <div className="px-3 pb-2.5 ml-6">
                        <div className="rounded-md bg-white/[0.03] border border-white/[0.06] p-2.5">
                          <p className="text-[11px] text-slate-400 mb-1 font-medium">Fix suggestion:</p>
                          <p className="text-[11px] text-slate-200/80">{finding.fixSuggestion}</p>
                          {finding.autoFixable && onAutoFix && (
                            <button
                              onClick={(e) => {
                                e.stopPropagation();
                                onAutoFix(finding.id);
                              }}
                              className="mt-2 flex items-center gap-1.5 text-[11px] text-glow-cyan font-medium hover:text-glow-cyan/80 transition-colors"
                            >
                              <Wrench className="h-3 w-3" />
                              Apply auto-fix
                            </button>
                          )}
                        </div>
                      </div>
                    )}
                  </div>
                );
              })}
            </div>
          );
        })}
      </div>
    </div>
  );
}
