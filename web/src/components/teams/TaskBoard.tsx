"use client";

import { useState } from "react";
import { cn } from "@/lib/utils";
import { X } from "lucide-react";
import type { TeamTask } from "@/hooks/useTeamExecution";

interface Props {
  tasks: TeamTask[];
}

// ---------------------------------------------------------------------------
// Column definitions
// ---------------------------------------------------------------------------

const COLUMNS: { key: TeamTask["status"]; label: string; color: string }[] = [
  { key: "backlog", label: "Backlog", color: "text-slate-400" },
  { key: "ready", label: "Ready", color: "text-blue-300" },
  { key: "in_progress", label: "In Progress", color: "text-blue-400" },
  { key: "in_review", label: "In Review", color: "text-amber-400" },
  { key: "done", label: "Done", color: "text-emerald-400" },
  { key: "failed", label: "Failed", color: "text-red-400" },
];

const PRIORITY_BADGE: Record<TeamTask["priority"], { label: string; className: string }> = {
  low: { label: "Low", className: "bg-slate-600/20 text-slate-400" },
  medium: { label: "Med", className: "bg-blue-500/10 text-blue-400" },
  high: { label: "High", className: "bg-amber-500/10 text-amber-400" },
  critical: { label: "Crit", className: "bg-red-500/10 text-red-400" },
};

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export function TaskBoard({ tasks }: Props) {
  const [selectedTask, setSelectedTask] = useState<TeamTask | null>(null);

  if (tasks.length === 0) {
    return (
      <div className="flex items-center justify-center h-full text-slate-400/50 text-sm">
        No tasks yet
      </div>
    );
  }

  // Group tasks by status
  const grouped = COLUMNS.map((col) => ({
    ...col,
    tasks: tasks.filter((t) => t.status === col.key),
  }));

  // Only show columns that have tasks or are key columns
  const visibleColumns = grouped.filter(
    (col) => col.tasks.length > 0 || col.key === "in_progress" || col.key === "done"
  );

  return (
    <div className="relative h-full flex flex-col">
      <div className="px-3 py-2 border-b border-white/[0.06]">
        <h3 className="text-[11px] text-slate-400 uppercase tracking-wider">
          Task Board
        </h3>
      </div>

      <div className="flex-1 overflow-x-auto overflow-y-auto scrollbar-thin">
        <div className="flex gap-2 p-3 min-w-max h-full">
          {visibleColumns.map((col) => (
            <div
              key={col.key}
              className="w-[180px] flex-shrink-0 flex flex-col"
            >
              {/* Column header */}
              <div className="flex items-center gap-1.5 mb-2 px-1">
                <span className={cn("text-[11px] font-medium uppercase tracking-wider", col.color)}>
                  {col.label}
                </span>
                <span className="text-[10px] text-slate-400/40">
                  {col.tasks.length}
                </span>
              </div>

              {/* Task cards */}
              <div className="flex-1 space-y-1.5">
                {col.tasks.map((task) => {
                  const priority = PRIORITY_BADGE[task.priority] ?? PRIORITY_BADGE.medium;
                  return (
                    <button
                      key={task.id}
                      onClick={() => setSelectedTask(task)}
                      className="w-full text-left p-2.5 rounded-lg border border-white/[0.06] bg-white/[0.02] hover:bg-white/[0.04] hover:border-white/[0.1] transition-all"
                    >
                      <p className="text-[12px] font-medium text-slate-200 line-clamp-2 mb-1.5">
                        {task.title}
                      </p>
                      <div className="flex items-center gap-1.5">
                        <span className={cn("text-[9px] px-1.5 py-0.5 rounded", priority.className)}>
                          {priority.label}
                        </span>
                        {task.assignee && (
                          <span className="text-[10px] text-slate-400 truncate">
                            {task.assignee}
                          </span>
                        )}
                      </div>
                    </button>
                  );
                })}
              </div>
            </div>
          ))}
        </div>
      </div>

      {/* Task detail overlay */}
      {selectedTask && (
        <div className="absolute inset-0 bg-black/50 backdrop-blur-sm flex items-center justify-center z-10 p-4">
          <div className="w-full max-w-sm rounded-xl border border-white/[0.1] bg-[#111118] p-4 space-y-3">
            <div className="flex items-start justify-between">
              <h4 className="text-sm font-semibold text-slate-200 flex-1 mr-2">
                {selectedTask.title}
              </h4>
              <button
                onClick={() => setSelectedTask(null)}
                className="text-slate-400 hover:text-slate-200 transition-colors p-0.5"
              >
                <X className="w-4 h-4" />
              </button>
            </div>
            <p className="text-xs text-slate-400">{selectedTask.description}</p>
            <div className="flex items-center gap-2 text-[11px]">
              <span className={cn("px-1.5 py-0.5 rounded", PRIORITY_BADGE[selectedTask.priority].className)}>
                {PRIORITY_BADGE[selectedTask.priority].label}
              </span>
              {selectedTask.assignee && (
                <span className="text-slate-400">Assigned to {selectedTask.assignee}</span>
              )}
            </div>
            {selectedTask.output && (
              <div className="mt-2">
                <label className="text-[10px] text-slate-400 uppercase tracking-wider block mb-1">
                  Output
                </label>
                <pre className="text-[11px] text-slate-200/80 bg-white/[0.03] rounded-md p-2 overflow-auto max-h-40 scrollbar-thin whitespace-pre-wrap">
                  {selectedTask.output}
                </pre>
              </div>
            )}
          </div>
        </div>
      )}
    </div>
  );
}
