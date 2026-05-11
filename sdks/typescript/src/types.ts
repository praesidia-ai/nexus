/** Result of a full app generation via /oneshot. */
export interface GenerationResult {
  /** Unique identifier for the generated project. */
  project_id: string;
  /** Human-readable project name. */
  project_name: string;
  /** Taste quality score (0.0 to 1.0). */
  taste_score: number;
  /** Number of files generated. */
  files_count: number;
  /** Total generation time in milliseconds. */
  duration_ms: number;
  /** URL where the running app can be accessed, if available. */
  app_url?: string;
}

/** Result of running a single agent on a task. */
export interface AgentResult {
  /** The agent's textual output. */
  output: string;
  /** Files that were created or modified by the agent. */
  files_modified: string[];
  /** Number of LLM iterations the agent used. */
  iterations_used: number;
  /** Total execution time in milliseconds. */
  duration_ms: number;
}

/** Result of running a multi-agent team on a task. */
export interface TeamResult {
  /** Artifacts produced by the team (file paths, URLs, etc.). */
  artifacts: string[];
  /** Total number of inter-agent messages exchanged. */
  messages_count: number;
  /** Estimated USD cost of all LLM calls. */
  cost_usd: number;
  /** Total execution time in milliseconds. */
  duration_ms: number;
}

/** Quality assessment of a generated project. */
export interface QualityScore {
  /** Overall quality score (0.0 to 1.0). */
  overall: number;
  /** Per-axis scores (e.g. "layout", "typography", "color"). */
  axes: Record<string, number>;
}

/** Result of intent analysis on a natural-language description. */
export interface IntentResult {
  /** Detected application type (e.g. "dashboard", "e-commerce"). */
  app_type: string;
  /** Estimated complexity (e.g. "simple", "medium", "complex"). */
  complexity: string;
  /** Application domain (e.g. "finance", "health"). */
  domain: string;
  /** Confidence of the intent analysis (0.0 to 1.0). */
  confidence: number;
}

/** A single entry from the persistent memory system. */
export interface MemoryEntry {
  /** Unique identifier for this memory entry. */
  id: string;
  /** The stored content. */
  content: string;
  /** Category or classification of the memory. */
  category: string;
  /** Confidence or relevance score (0.0 to 1.0). */
  confidence: number;
}

/** An A2A Agent Card describing a discoverable agent. */
export interface AgentCard {
  name: string;
  description: string;
  url: string;
  version: string;
  capabilities: {
    streaming?: boolean;
    push_notifications?: boolean;
    state_transition_history?: boolean;
  };
  default_input_modes: string[];
  default_output_modes: string[];
  skills: Array<{
    id: string;
    name: string;
    description: string;
    tags: string[];
  }>;
}

/** A marketplace package entry. */
export interface MarketplacePackage {
  name: string;
  kind: 'agent' | 'plugin' | 'workflow' | 'tool';
  runtime: 'wasm' | 'a2a' | 'native';
  latestVersion: string;
  description: string;
  keywords: string[];
  categories: string[];
  downloads: number;
  rating: number | null;
  reviewCount: number;
  author: string;
  installed: boolean;
  installedVersion?: string;
}

/** A governance policy. */
export interface GovernancePolicy {
  id: string;
  name: string;
  description: string;
  enabled: boolean;
  rules: Array<{
    kind: string;
    pattern?: string;
    message: string;
    block: boolean;
  }>;
}

/** Result of evaluating an action against governance policies. */
export interface EvaluationResult {
  allowed: boolean;
  matched_policies: string[];
  violations: string[];
  warnings: string[];
}

/** A cryptographic audit log entry. */
export interface AuditEntry {
  index: number;
  actor_id: string;
  action: string;
  resource: string;
  outcome: string;
  timestamp: string;
  prev_hash: string;
  entry_hash: string;
  signature: string;
}

/** A workflow run. */
export interface WorkflowRun {
  id: string;
  name: string;
  status: 'pending' | 'running' | 'completed' | 'failed' | 'cancelled';
  started_at?: string;
  finished_at?: string;
  steps: Array<{
    id: string;
    name: string;
    status: string;
  }>;
}
