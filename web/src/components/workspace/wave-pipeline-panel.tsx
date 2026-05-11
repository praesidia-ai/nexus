"use client";

import { Loader2, CheckCircle2, XCircle, Circle, ChevronDown, ChevronRight } from "lucide-react";
import { cn } from "@/lib/utils";
import { useState } from "react";
import type { WavePipelineState, WaveAgentStatus } from "@/lib/wave-pipeline-types";
import { WAVE_COLORS } from "@/lib/wave-pipeline-types";

interface WavePipelinePanelProps {
  state: WavePipelineState;
}

export function WavePipelinePanel({ state }: WavePipelinePanelProps) {
  const [expandedWaves, setExpandedWaves] = useState<Set<number>>(new Set());

  const toggleWave = (idx: number) => {
    setExpandedWaves((prev) => {
      const next = new Set(prev);
      if (next.has(idx)) next.delete(idx);
      else next.add(idx);
      return next;
    });
  };

  // Group agents by wave
  const agentsByWave: Record<number, WaveAgentStatus[]> = {};
  for (const agent of Object.values(state.agents)) {
    if (!agentsByWave[agent.wave]) agentsByWave[agent.wave] = [];
    agentsByWave[agent.wave].push(agent);
  }

  // Get sorted wave indexes
  const waveIndexes = Object.keys(state.waves).map(Number).sort((a, b) => a - b);
  if (waveIndexes.length === 0 && !state.running) return null;

  // Stats
  const totalAgents = Object.keys(state.agents).length;
  const completedAgents = Object.values(state.agents).filter((a) => a.status === "completed").length;
  const failedAgents = Object.values(state.agents).filter((a) => a.status === "failed").length;
  const runningAgents = Object.values(state.agents).filter((a) => a.status === "running").length;

  return (
    <div className="space-y-2">
      {/* Header */}
      <div className="flex items-center justify-between">
        <span className="text-[11px] font-semibold uppercase tracking-wider text-white/40">
          Wave Pipeline
        </span>
        <div className="flex items-center gap-2">
          {state.running && (
            <span className="flex items-center gap-1.5 text-[10px] text-emerald-400">
              <span className="w-1.5 h-1.5 rounded-full bg-emerald-400 animate-pulse" />
              {runningAgents} running
            </span>
          )}
          {totalAgents > 0 && (
            <span className="text-[10px] text-white/30">
              {completedAgents}/{totalAgents} done
            </span>
          )}
        </div>
      </div>

      {/* Progress bar */}
      {totalAgents > 0 && (
        <div className="h-1.5 bg-white/[0.06] rounded-full overflow-hidden">
          <div className="h-full flex">
            <div
              className="bg-emerald-500 transition-all duration-500"
              style={{ width: `${(completedAgents / totalAgents) * 100}%` }}
            />
            {failedAgents > 0 && (
              <div
                className="bg-red-500 transition-all duration-500"
                style={{ width: `${(failedAgents / totalAgents) * 100}%` }}
              />
            )}
            {runningAgents > 0 && (
              <div
                className="bg-violet-500 animate-pulse transition-all duration-500"
                style={{ width: `${(runningAgents / totalAgents) * 100}%` }}
              />
            )}
          </div>
        </div>
      )}

      {/* Waves */}
      <div className="space-y-1">
        {waveIndexes.map((waveIdx) => {
          const wave = state.waves[waveIdx];
          if (!wave) return null;
          const colors = WAVE_COLORS[waveIdx] ?? WAVE_COLORS[0];
          const agents = agentsByWave[waveIdx] ?? [];
          const isExpanded = expandedWaves.has(waveIdx) || wave.status === "running";

          return (
            <div key={waveIdx}>
              {/* Wave header */}
              <button
                onClick={() => toggleWave(waveIdx)}
                className={cn(
                  "w-full flex items-center gap-2 px-2.5 py-1.5 rounded-lg border transition-all text-left",
                  wave.status === "running"
                    ? `${colors.border} ${colors.bg}`
                    : wave.status === "completed"
                      ? "border-emerald-500/20 bg-emerald-500/5"
                      : wave.status === "failed"
                        ? "border-red-500/20 bg-red-500/5"
                        : "border-white/[0.06] bg-white/[0.02]"
                )}
              >
                {/* Status icon */}
                {wave.status === "running" ? (
                  <Loader2 className={cn("w-3 h-3 animate-spin shrink-0", colors.text)} />
                ) : wave.status === "completed" ? (
                  <CheckCircle2 className="w-3 h-3 text-emerald-400 shrink-0" />
                ) : wave.status === "failed" ? (
                  <XCircle className="w-3 h-3 text-red-400 shrink-0" />
                ) : (
                  <Circle className="w-3 h-3 text-white/15 shrink-0" />
                )}

                {/* Expand arrow */}
                {isExpanded ? (
                  <ChevronDown className="w-3 h-3 text-white/30 shrink-0" />
                ) : (
                  <ChevronRight className="w-3 h-3 text-white/30 shrink-0" />
                )}

                {/* Wave label */}
                <span className={cn(
                  "text-[11px] font-medium flex-1",
                  wave.status === "running" ? colors.text
                    : wave.status === "completed" ? "text-emerald-400/80"
                    : wave.status === "failed" ? "text-red-400/80"
                    : "text-white/25"
                )}>
                  W{waveIdx}: {wave.label}
                </span>

                {/* Agent count */}
                <span className="text-[9px] text-white/20">
                  {wave.agentsSucceeded}/{wave.agentCount}
                </span>

                {/* Duration */}
                {wave.durationMs != null && wave.durationMs > 0 && (
                  <span className="text-[9px] text-white/15">
                    {(wave.durationMs / 1000).toFixed(1)}s
                  </span>
                )}
              </button>

              {/* Expanded agents list */}
              {isExpanded && agents.length > 0 && (
                <div className="ml-4 mt-0.5 space-y-0.5">
                  {agents.map((agent) => (
                    <div
                      key={agent.agentId}
                      className={cn(
                        "flex items-center gap-1.5 px-2 py-0.5 rounded text-[10px]",
                        agent.status === "running" ? "text-white/60"
                          : agent.status === "completed" ? "text-emerald-400/60"
                          : agent.status === "failed" ? "text-red-400/60"
                          : "text-white/20"
                      )}
                    >
                      {agent.status === "running" ? (
                        <Loader2 className="w-2.5 h-2.5 animate-spin shrink-0" />
                      ) : agent.status === "completed" ? (
                        <CheckCircle2 className="w-2.5 h-2.5 shrink-0" />
                      ) : agent.status === "failed" ? (
                        <XCircle className="w-2.5 h-2.5 shrink-0" />
                      ) : (
                        <Circle className="w-2.5 h-2.5 shrink-0" />
                      )}

                      <span className="truncate flex-1">{agent.label}</span>

                      {agent.status === "running" && agent.iteration && (
                        <span className="text-[8px] text-white/20">
                          {agent.iteration.num}/{agent.iteration.max}
                        </span>
                      )}

                      {agent.status === "completed" && agent.toolCalls > 0 && (
                        <span className="text-[8px] text-white/15">
                          {agent.toolCalls}t {agent.filesChanged}f
                        </span>
                      )}
                    </div>
                  ))}
                </div>
              )}
            </div>
          );
        })}
      </div>

      {/* Summary stats */}
      {totalAgents > 0 && (
        <div className="flex items-center gap-3 pt-1.5 border-t border-white/[0.06] text-[10px] text-white/30">
          <span><span className="text-emerald-400 font-medium">{completedAgents}</span> ok</span>
          {failedAgents > 0 && <span><span className="text-red-400 font-medium">{failedAgents}</span> fail</span>}
          <span>
            <span className="text-white/50 font-medium">
              {Object.values(state.agents).reduce((s, a) => s + a.toolCalls, 0)}
            </span> tools
          </span>
          <span>
            <span className="text-white/50 font-medium">
              {Object.values(state.agents).reduce((s, a) => s + a.filesChanged, 0)}
            </span> files
          </span>
        </div>
      )}
    </div>
  );
}
