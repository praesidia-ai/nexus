"use client";

import { cn } from "@/lib/utils";
import { Bot, Loader2, CheckCircle2, XCircle, Clock } from "lucide-react";
import type { MemberStatus } from "@/hooks/useTeamExecution";

interface Props {
  members: MemberStatus[];
}

const STATUS_CONFIG: Record<
  MemberStatus["status"],
  { color: string; bgColor: string; icon: typeof Bot; label: string }
> = {
  idle: { color: "text-slate-400", bgColor: "bg-slate-600", icon: Clock, label: "Idle" },
  working: { color: "text-blue-400", bgColor: "bg-blue-500/20", icon: Loader2, label: "Working" },
  done: { color: "text-emerald-400", bgColor: "bg-emerald-500/20", icon: CheckCircle2, label: "Done" },
  failed: { color: "text-red-400", bgColor: "bg-red-500/20", icon: XCircle, label: "Failed" },
  waiting: { color: "text-slate-400/60", bgColor: "bg-white/[0.04]", icon: Clock, label: "Waiting" },
};

export function MemberStatusPanel({ members }: Props) {
  if (members.length === 0) {
    return (
      <div className="flex items-center justify-center h-full text-slate-400/50 text-sm">
        No members yet
      </div>
    );
  }

  return (
    <div className="space-y-2 p-3">
      <h3 className="text-[11px] text-slate-400 uppercase tracking-wider px-1 mb-2">
        Team Members
      </h3>
      {members.map((member) => {
        const config = STATUS_CONFIG[member.status] ?? STATUS_CONFIG.idle;
        const StatusIcon = config.icon;

        return (
          <div
            key={member.id}
            className="p-3 rounded-lg border border-white/[0.06] bg-white/[0.02] space-y-2"
          >
            {/* Header */}
            <div className="flex items-center gap-2">
              <div
                className={cn(
                  "w-7 h-7 rounded-lg flex items-center justify-center flex-shrink-0",
                  config.bgColor
                )}
              >
                <span className={cn("text-xs font-bold", config.color)}>
                  {member.name[0]?.toUpperCase() ?? "?"}
                </span>
              </div>
              <div className="flex-1 min-w-0">
                <div className="flex items-center gap-1.5">
                  <span className="text-[13px] font-medium text-slate-200 truncate">
                    {member.name}
                  </span>
                  <StatusIcon
                    className={cn(
                      "w-3 h-3 flex-shrink-0",
                      config.color,
                      member.status === "working" && "animate-spin"
                    )}
                  />
                </div>
                <span className="text-[11px] text-slate-400">{member.role}</span>
              </div>
              <span className={cn("text-[10px] font-medium", config.color)}>
                {config.label}
              </span>
            </div>

            {/* Progress bar */}
            <div className="h-1.5 rounded-full bg-white/[0.04] overflow-hidden">
              <div
                className={cn(
                  "h-full rounded-full transition-all duration-500",
                  member.status === "done"
                    ? "bg-emerald-500"
                    : member.status === "failed"
                    ? "bg-red-500"
                    : member.status === "working"
                    ? "bg-blue-500"
                    : "bg-slate-600"
                )}
                style={{ width: `${member.progress}%` }}
              />
            </div>

            {/* Current activity */}
            {member.currentTask && (
              <p className="text-[11px] text-slate-400 truncate">
                {member.currentTask}
              </p>
            )}

            {/* Stats */}
            <div className="flex items-center gap-3 text-[10px] text-slate-400/60">
              <span>{member.tokensUsed.toLocaleString()} tokens</span>
              <span>${member.cost.toFixed(4)}</span>
              <span className="text-[10px]">{member.model}</span>
            </div>
          </div>
        );
      })}
    </div>
  );
}
