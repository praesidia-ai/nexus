"use client";

import { useParams, useRouter } from "next/navigation";
import {
  Clock,
  DollarSign,
  FileText,
  CheckCircle2,
  Loader2,
  Pause,
  XCircle,
  ArrowLeft,
} from "lucide-react";
import { cn } from "@/lib/utils";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { useTeamExecution } from "@/hooks/useTeamExecution";
import { MemberStatusPanel } from "@/components/teams/MemberStatusPanel";
import { CommunicationFeed } from "@/components/teams/CommunicationFeed";
import { TaskBoard } from "@/components/teams/TaskBoard";
import { HumanInput } from "@/components/teams/HumanInput";

// ---------------------------------------------------------------------------
// Status badge mapping
// ---------------------------------------------------------------------------

const STATUS_DISPLAY: Record<
  string,
  { label: string; icon: typeof Loader2; className: string }
> = {
  idle: { label: "Idle", icon: Clock, className: "text-slate-400 border-muted-foreground/20" },
  running: { label: "Running", icon: Loader2, className: "text-blue-400 border-blue-500/20 bg-blue-500/[0.06]" },
  paused: { label: "Paused", icon: Pause, className: "text-amber-400 border-amber-500/20 bg-amber-500/[0.06]" },
  completed: { label: "Completed", icon: CheckCircle2, className: "text-emerald-400 border-emerald-500/20 bg-emerald-500/[0.06]" },
  failed: { label: "Failed", icon: XCircle, className: "text-red-400 border-red-500/20 bg-red-500/[0.06]" },
};

function formatDuration(seconds: number): string {
  const m = Math.floor(seconds / 60);
  const s = seconds % 60;
  return m > 0 ? `${m}m ${s}s` : `${s}s`;
}

// ---------------------------------------------------------------------------
// Page
// ---------------------------------------------------------------------------

export default function TeamExecutionPage() {
  const params = useParams();
  const router = useRouter();
  const projectId = params.projectId as string;
  const teamId = params.teamId as string;
  const runId = params.runId as string;

  const { state, inject, pause, resume } = useTeamExecution(runId);

  const statusDisplay = STATUS_DISPLAY[state.status] ?? STATUS_DISPLAY.idle;
  const StatusIcon = statusDisplay.icon;

  return (
    <div className="h-full flex flex-col overflow-hidden">
      {/* Header bar */}
      <div className="flex-shrink-0 px-4 py-3 border-b border-white/[0.06] bg-white/[0.01] flex items-center gap-4">
        <Button
          variant="ghost"
          size="sm"
          onClick={() => router.push(`/${projectId}/teams`)}
          className="text-slate-400"
        >
          <ArrowLeft className="w-4 h-4" />
        </Button>

        <div className="flex-1 min-w-0">
          <div className="flex items-center gap-2">
            <h1 className="text-sm font-semibold text-slate-200 truncate">
              {state.task || "Team Execution"}
            </h1>
            <Badge
              variant="outline"
              className={cn("text-[10px] gap-1", statusDisplay.className)}
            >
              <StatusIcon
                className={cn(
                  "w-3 h-3",
                  state.status === "running" && "animate-spin"
                )}
              />
              {statusDisplay.label}
            </Badge>
          </div>
          <p className="text-[11px] text-slate-400 mt-0.5">
            Team {state.teamId || teamId}
          </p>
        </div>

        {/* Timer */}
        <div className="flex items-center gap-1.5 text-xs text-slate-400">
          <Clock className="w-3.5 h-3.5" />
          <span className="tabular-nums font-medium">{formatDuration(state.duration)}</span>
        </div>

        {/* Budget gauge */}
        <div className="flex items-center gap-2">
          <DollarSign className="w-3.5 h-3.5 text-slate-400" />
          <div className="w-32">
            <div className="flex items-center justify-between text-[10px] text-slate-400 mb-0.5">
              <span>${state.budget.spent.toFixed(2)}</span>
              <span>{state.budget.percentUsed.toFixed(0)}%</span>
            </div>
            <div className="h-1.5 rounded-full bg-white/[0.06] overflow-hidden">
              <div
                className={cn(
                  "h-full rounded-full transition-all duration-500",
                  state.budget.percentUsed > 90
                    ? "bg-red-500"
                    : state.budget.percentUsed > 70
                    ? "bg-amber-500"
                    : "bg-emerald-500"
                )}
                style={{ width: `${Math.min(state.budget.percentUsed, 100)}%` }}
              />
            </div>
          </div>
        </div>

        {/* Artifacts count */}
        <div className="flex items-center gap-1.5 text-xs text-slate-400">
          <FileText className="w-3.5 h-3.5" />
          <span>{state.artifacts.length}</span>
        </div>

        {/* Results button */}
        {(state.status === "completed" || state.status === "failed") && (
          <Button
            size="sm"
            onClick={() =>
              router.push(`/${projectId}/teams/${teamId}/runs/${runId}/results`)
            }
            className="bg-glow-cyan/10 text-glow-cyan hover:bg-glow-cyan/20 border border-primary/20"
          >
            View Results
          </Button>
        )}
      </div>

      {/* 3-panel layout */}
      <div className="flex-1 flex overflow-hidden">
        {/* Left panel: Member status */}
        <div className="w-[260px] flex-shrink-0 border-r border-white/[0.06] overflow-y-auto scrollbar-thin">
          <MemberStatusPanel members={state.members} />
        </div>

        {/* Center panel: Communication feed */}
        <div className="flex-1 min-w-0 border-r border-white/[0.06]">
          <CommunicationFeed messages={state.messages} />
        </div>

        {/* Right panel: Task board */}
        <div className="w-[320px] flex-shrink-0 overflow-hidden">
          <TaskBoard tasks={state.tasks} />
        </div>
      </div>

      {/* Bottom bar: Human input */}
      <HumanInput
        members={state.members}
        status={state.status}
        onInject={inject}
        onPause={pause}
        onResume={resume}
      />
    </div>
  );
}
