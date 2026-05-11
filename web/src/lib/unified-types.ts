// Unified workspace types — merges Chat, Plan, and Agent into one timeline.

export type TimelineItemType =
  | "user_message"
  | "assistant_message"
  | "thinking"
  | "plan_block"
  | "agent_tool"
  | "agent_phase"
  | "agent_done"
  | "agent_proposal"
  | "action_result"
  | "error"
  | "milestone"
  | "hitl_gate"
  | "parallel_gate";

export type AgentRoleType =
  | "architect"
  | "coder"
  | "reviewer"
  | "tester"
  | "debugger"
  | "performance"
  | "ux"
  | "devops"
  | "refactor"
  | "product";

export type AgentPhaseType =
  | "analyze"
  | "plan"
  | "implement"
  | "review"
  | "test"
  | "debug"
  | "verify"
  | "optimize"
  | "ux"
  | "deploy"
  | "refactor"
  | "product"
  | "complete"
  | "failed";

export type PipelineMode =
  | "pipeline"
  | "full_pipeline"
  | "architect"
  | "coder"
  | "review"
  | "debug"
  | "performance"
  | "ux"
  | "devops"
  | "refactor"
  | "product";

/** Metadata for each agent role, used for UI rendering */
export const AGENT_METADATA: Record<AgentRoleType, { label: string; icon: string; color: string; description: string }> = {
  architect:   { label: "Architect",    icon: "Compass",       color: "violet",  description: "Plans and designs implementation" },
  coder:       { label: "Coder",        icon: "Code2",         color: "blue",    description: "Writes and edits code" },
  reviewer:    { label: "Reviewer",     icon: "ShieldCheck",   color: "amber",   description: "Reviews code quality and security" },
  tester:      { label: "Tester",       icon: "TestTube2",     color: "green",   description: "Writes and runs tests" },
  debugger:    { label: "Debugger",     icon: "Bug",           color: "red",     description: "Diagnoses and fixes errors" },
  performance: { label: "Performance",  icon: "Gauge",         color: "orange",  description: "Optimizes speed and efficiency" },
  ux:          { label: "UX",           icon: "Palette",       color: "pink",    description: "Improves UI and accessibility" },
  devops:      { label: "DevOps",       icon: "Container",     color: "cyan",    description: "Manages build and deployment" },
  refactor:    { label: "Refactor",     icon: "Recycle",       color: "teal",    description: "Cleans and restructures code" },
  product:     { label: "Product",      icon: "Lightbulb",     color: "yellow",  description: "Improves features and flows" },
};

export interface TimelineItem {
  id: string;
  type: TimelineItemType;
  timestamp: number;
  // Message content
  content?: string;
  streaming?: boolean;
  // Plan block
  plan?: PlanData;
  // Agent tool call
  toolName?: string;
  toolArgs?: Record<string, unknown>;
  toolOutput?: string;
  toolSuccess?: boolean;
  toolDuration?: number;
  // Agent identity (which agent produced this event)
  agentRole?: AgentRoleType;
  // Agent phase
  phaseName?: string;
  phaseStatus?: string;
  // Parallel gate (multiple agents running concurrently)
  parallelAgents?: { role: AgentRoleType; status: "running" | "completed" | "failed" }[];
  // Action result (from chat actions)
  action?: string;
  actionData?: Record<string, unknown>;
  actionSuccess?: boolean;
  // Done summary
  summary?: string;
  totalIterations?: number;
  totalTools?: string[];
  // Error
  errorMessage?: string;
  // Milestone
  milestone?: string;
  // Phase label (from chat)
  phaseNumber?: number;
  phaseLabel?: string;
  // Proposal (HITL)
  proposalIntent?: string;
  proposalOperations?: { file?: string; action?: string; description?: string }[];
  proposalRiskLevel?: string;
  proposalWarnings?: string[];
  // HITL Gate
  gateId?: string;
  gateType?: string;
  gateResolved?: boolean;
}

export interface PlanData {
  summary?: string;
  architecture?: {
    framework?: string;
    database?: string;
    auth?: boolean;
  };
  entities?: {
    name: string;
    description?: string;
    fields: { name: string; type: string; required?: boolean }[];
    relationships?: { target: string; type: string }[];
  }[];
  agents?: {
    name: string;
    role: string;
    tools?: string[];
  }[];
  pages?: {
    name: string;
    route: string;
    description?: string;
    components?: string[];
  }[];
  integrations?: string[];
}

export type WorkspaceMode = "chat" | "agent" | "auto";

export interface WorkspaceState {
  mode: WorkspaceMode;
  timeline: TimelineItem[];
  streaming: boolean;
  conversationId?: string;
  currentPlan?: PlanData;
  // Agent state
  agentRunning: boolean;
  currentIteration?: { num: number; max: number };
  currentPhase?: string;
  currentAgent?: AgentRoleType;
  // Multi-agent tracking
  pipelineMode?: PipelineMode;
  activeAgents: Record<string, AgentRunStatus>;
  // Processing steps (chat actions in-flight)
  processingSteps: { action: string; data: Record<string, unknown>; done: boolean }[];
  // Model selection
  provider?: string;
  model?: string;
}

export interface AgentRunStatus {
  role: AgentRoleType;
  status: "idle" | "running" | "completed" | "failed";
  iteration?: { num: number; max: number };
  toolCalls: number;
  filesChanged: number;
  startedAt?: number;
  completedAt?: number;
  error?: string;
}

export interface AgentStatus {
  name: string;
  status: "idle" | "running" | "completed" | "error";
  currentTool?: string;
}

export interface HitlGate {
  id: string;
  gate_type: string;
  status: string;
  description?: string;
  context?: Record<string, unknown>;
  created_at: string;
}

export interface CostSummary {
  total_cost_usd: number;
  by_model: { model: string; cost_usd: number; tokens: number }[];
  today_cost_usd: number;
}

export interface CostBudget {
  daily_limit_usd: number;
  used_today_usd: number;
  remaining_usd: number;
  percentage_used: number;
}
