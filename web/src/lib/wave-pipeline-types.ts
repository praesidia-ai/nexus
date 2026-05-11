// Wave Pipeline types — 50+ parallel mini-agents organized into execution waves.

export interface MiniAgentDef {
  id: string;
  label: string;
  description: string;
  wave: number;
  wave_label: string;
  icon: string;
  color: string;
  tools: string[];
  max_iterations: number;
  fatal: boolean;
  writes_files: boolean;
  tags: string[];
}

export interface WaveDef {
  index: number;
  label: string;
  description: string;
  agents: string[];
}

export interface WaveAgentStatus {
  agentId: string;
  label: string;
  icon: string;
  color: string;
  wave: number;
  status: "idle" | "running" | "completed" | "failed";
  iteration?: { num: number; max: number };
  toolCalls: number;
  filesChanged: number;
  summary?: string;
  durationMs?: number;
  startedAt?: number;
  completedAt?: number;
}

export interface WaveStatus {
  index: number;
  label: string;
  status: "idle" | "running" | "completed" | "failed";
  agentCount: number;
  agentsSucceeded: number;
  agentsFailed: number;
  durationMs?: number;
}

export interface WavePipelineState {
  running: boolean;
  totalWaves: number;
  totalAgents: number;
  currentWave?: number;
  waves: Record<number, WaveStatus>;
  agents: Record<string, WaveAgentStatus>;
}

// SSE event types from the wave pipeline
export type WaveEventType =
  | "wave_pipeline_start"
  | "wave_start"
  | "wave_agent_start"
  | "wave_agent_tool"
  | "wave_agent_tool_result"
  | "wave_agent_iteration"
  | "wave_agent_done"
  | "wave_done"
  | "wave_agent_thinking"
  | "wave_file_change"
  | "wave_pipeline_done"
  | "wave_error";

// Color map for wave indexes
export const WAVE_COLORS: Record<number, { bg: string; border: string; text: string; label: string }> = {
  0: { bg: "bg-violet-500/10", border: "border-violet-500/30", text: "text-violet-400", label: "Analyze" },
  1: { bg: "bg-blue-500/10", border: "border-blue-500/30", text: "text-blue-400", label: "Plan" },
  2: { bg: "bg-sky-500/10", border: "border-sky-500/30", text: "text-sky-400", label: "Implement" },
  3: { bg: "bg-amber-500/10", border: "border-amber-500/30", text: "text-amber-400", label: "Quality" },
  4: { bg: "bg-green-500/10", border: "border-green-500/30", text: "text-green-400", label: "Test" },
  5: { bg: "bg-yellow-500/10", border: "border-yellow-500/30", text: "text-yellow-400", label: "Polish" },
  6: { bg: "bg-cyan-500/10", border: "border-cyan-500/30", text: "text-cyan-400", label: "DevOps" },
};
