// In the browser, use the Next.js proxy at /api.
// In Node.js (server components), go directly to the API server.
// Default backend dev port is 8020; frontend dev/prod port is 8080.
export const BASE =
  typeof window === "undefined"
    ? (process.env.NEXUS_API_URL ?? "http://localhost:8020")
    : "/api";

export interface Project {
  id: string;
  name: string;
  description?: string;
  phase: number;
  llm_provider?: string;
  llm_model?: string;
  created_at: string;
  updated_at: string;
}

export interface Conversation {
  id: string;
  project_id: string;
  created_at: string;
}

export interface NexusMessage {
  id: string;
  conversation_id: string;
  role: "user" | "assistant" | "system";
  content: string;
  created_at: string;
}

export interface KnowledgeItem {
  id: string;
  project_id: string;
  item_type: string;
  name: string;
  description?: string;
  icon?: string;
  metadata?: unknown;
  created_at: string;
}

export interface MaterializedTable {
  id: string;
  project_id: string;
  table_name: string;
  schema_json: unknown;
  created_at: string;
}

export interface AgentDefinition {
  id: string;
  project_id: string;
  name: string;
  role: string;
  tools: string[];
  memory_type: string;
  provider: string;
  model: string;
  system_prompt: string;
  status: string;
  zeroclaw_pid?: number;
  zeroclaw_port?: number;
  persona?: string;
  custom_rules?: string[];
  temperature?: number;
  max_tokens?: number;
  created_at: string;
  updated_at: string;
}

export interface AgentSkill {
  id: string;
  project_id?: string;
  name: string;
  description: string;
  icon: string;
  category: string;
  system_prompt: string;
  tools: string[];
  rules: string[];
  examples: { user: string; assistant: string }[];
  temperature: number;
  max_tokens: number;
  is_builtin: boolean;
  created_at: string;
  updated_at: string;
}

export interface SkillAssignment {
  id: string;
  agent_id: string;
  skill_id: string;
  priority: number;
  created_at: string;
}

export interface DesignedWorkflowAgent {
  temp_id: string;
  name: string;
  role: string;
  description: string;
  system_prompt: string;
  tools: string[];
  model_suggestion: string;
  trigger: string;
  icon: string;
}

export interface WorkflowConnection {
  from_agent: string;
  to_agent: string;
  condition: string;
  data_passed: string;
}

export interface DesignedWorkflow {
  title: string;
  description: string;
  agents: DesignedWorkflowAgent[];
  connections: WorkflowConnection[];
  execution_mode: string;
  estimated_complexity: string;
  tags: string[];
}

export interface AppInstance {
  id: string;
  project_id: string;
  port: number;
  pid?: number;
  status: string;
  output_dir: string;
  error?: string;
  started_at: string;
  stopped_at?: string;
  sandbox: boolean;
  container_id?: string;
  logs?: string;
  health_status?: string;
  restart_count?: number;
  auto_restart?: boolean;
  url?: string;
}

export interface BuildStep {
  id: string;
  instance_id: string;
  step_name: string;
  label: string;
  status: string;
  started_at?: string;
  completed_at?: string;
  duration_ms?: number;
  output: string;
  error?: string;
}

export interface EnvVar {
  id: string;
  project_id: string;
  key: string;
  value: string;
  created_at: string;
}

export interface SandboxSettings {
  enabled: boolean;
  docker_available: boolean;
}

export interface CodingTask {
  id: string;
  project_id: string;
  title: string;
  description: string;
  agent: string;
  status: string;
  status_detail?: string;
  priority: number;
  plan_json?: Record<string, unknown>;
  files_json?: Record<string, unknown>[];
  output?: string;
  review_json?: { score?: number; issues?: { severity: string; description: string; fix?: string }[]; summary?: string };
  error?: string;
  started_at?: string;
  completed_at?: string;
  created_at: string;
  updated_at: string;
}

export interface CodeGenResult {
  project_id: string;
  output_dir: string;
  files_written: string[];
  tables_created: string[];
  agents_configured: string[];
  validation: { valid: boolean; errors: string[]; warnings: string[] };
  created_at: string;
}

export interface VaultEntry {
  id: string;
  project_id: string;
  key: string;
  value: string;
  created_at: string;
}

export interface WorkflowRun {
  id: string;
  project_id?: string;
  pipeline: string;
  status: string;
  input_json?: string;
  output_json?: string;
  error?: string;
  started_at: string;
  finished_at?: string;
}

export interface Portal {
  id: string;
  project_id: string;
  slug: string;
  project_name: string;
  agent_name: string;
  published_at?: string;
  created_at: string;
}

// ═══════════════════════════════════════════════════════════════════════════
// Intelligence Layer Types
// ═══════════════════════════════════════════════════════════════════════════

export interface BrainOutput {
  inferred_intent: unknown;
  hidden_requirements: HiddenRequirements;
  architecture_decisions: unknown;
  learning_notes: string[];
  decision_confidence: number;
  product_brief: unknown;
  ux_strategy: { primary_goal: string; principles: string[]; critical_flows: string[]; design_emphasis: string[] };
  monetization_summary: string;
  agent_plan: DesignedAgent[];
  risk_analysis: { risks: { area: string; description: string; severity: string; mitigation: string }[]; overall_level: string; auto_mitigations: string[] };
  taste_target_score: number;
  execution_strategy: { mode: string; skip_steps: string[]; auto_inject_agents: boolean; max_redesign_attempts: number };
  personality: string;
  explanations: { decision_id: string; decision: string; reason: string; confidence: number }[];
  global_intelligence: unknown;
  stack_suggestions: unknown[];
  amplification: unknown;
  inferred_domain: string;
  from_cache: boolean;
  analysis_ms: number;
}

export interface HiddenRequirements {
  needs_auth: boolean;
  needs_database: boolean;
  needs_payments: boolean;
  additional_features: string[];
  additional_pages: string[];
  additional_entities: string[];
  confidence: number;
  reasoning: string[];
}

export interface DesignedAgent {
  name: string;
  agent_type: string;
  description: string;
  system_prompt: string;
  tools: string[];
  trigger: unknown;
  ui_placement: string;
  api_route: string;
  priority: number;
}

export interface TasteGateResult {
  passed: boolean;
  final_score: { overall: number };
  target_score: number;
  attempts: number;
  score_history: number[];
  improvements_applied: string[];
  failure_explanation: string | null;
}

export interface AutonomousResult {
  analysis: { missing_features: string[]; ux_issues: string[]; performance_issues: string[]; total_issues: number };
  actions_executed: { action: string; category: string; success: boolean; detail: string; duration_ms: number }[];
  final_taste_score: number | null;
  duration_ms: number;
  fully_succeeded: boolean;
}

export interface Prediction {
  likely_next_actions: { action: string; confidence: number; category: string }[];
  confidence: number;
  preload_actions: { action: string; priority: number }[];
}

export interface ModelSelection {
  provider: string;
  model: string;
  reasoning: string;
  estimated_cost_per_1k: number;
  latency_tier: string;
}

export interface UnifiedMemory {
  global_patterns: { key: string; value: string; category: string; confidence: number; times_applied: number }[];
  project_patterns: string[];
  skill_patterns: { name: string; intent: string; confidence: number; total_uses: number; successes: number }[];
  success_rates: { category: string; rate: number; sample_size: number }[];
}

export interface StrategyPlan {
  suggested_features: FeatureSuggestion[];
  maturity: string;
  reasoning: string[];
}

export interface FeatureSuggestion {
  name: string;
  rationale: string;
  estimated_impact: number;
  estimated_effort: number;
  category: string;
  priority_score: number;
}

export interface CausalGraph {
  links: { cause: string; effect: string; strength: number; confidence: number; observations: number }[];
  total_observations: number;
  strongest_positive: unknown | null;
  strongest_negative: unknown | null;
}

export interface CollectiveDecision {
  proposal: unknown;
  critiques: { agent: string; verdict: string; issues: string[]; suggestions: string[] }[];
  final_decision: { summary: string; changes_from_proposal: string[]; final_pages: string[]; risk_level: string };
  confidence_score: number;
}

export interface AnticipationReport {
  patterns_detected: { pattern_type: string; description: string; occurrences: number }[];
  actions: { suggestion: string; confidence: number; auto_execute: boolean; reason: string; category: string }[];
}

export interface MarketplacePlugin {
  id: string;
  name: string;
  description: string;
  author: string;
  version: string;
  categories: string[];
  usage_count: number;
  rating: number;
  compatible_app_types: string[];
  provides: string[];
}

export interface PluginSuggestion {
  plugin: MarketplacePlugin;
  reason: string;
  relevance: number;
}

export interface ProjectIntelligence {
  architecture_history: { area: string; choice: string; outcome: string }[];
  successful_patterns: string[];
  failure_fixes: { error: string; fix_applied: string; worked: boolean }[];
  taste_history: [string, number][];
}

export interface RuntimeHealthReport {
  status: string;
  total_events: number;
  error_rate: number;
  crash_count: number;
  top_errors: { endpoint: string; error_count: number }[];
  recommendations: string[];
}

export interface TestResult {
  success: boolean;
  total: number;
  passed: number;
  failed: number;
  failures: { test_name: string; expected: string; actual: string; detail: string }[];
  coverage_estimate: number;
  duration_ms: number;
}

export interface PromptPerformance {
  purpose: string;
  latest_outcome: string;
  confidence: number;
  total_uses: number;
}

export interface KernelProcess {
  pid: string;
  agent_id?: string;
  agent_name: string;
  state: string; // ready | running | waiting | suspended | sleeping | completed | failed | killed
  priority: string | number;
  task: string;
  tokens_used: number;
  cost_used: number;
  iterations?: number;
  parent?: string | null;
  children?: string[];
  created_at?: string;
  updated_at?: string;
  started_at?: string | null;
  finished_at?: string | null;
}

export interface MemoryEntry {
  id: string;
  content: string;
  tags: string[];
  timestamp: string;
  relevance?: number;
}

// ═══════════════════════════════════════════════════════════════════════
// Governance types
// ═══════════════════════════════════════════════════════════════════════

export interface GovernancePolicy {
  id: string;
  name: string;
  description?: string;
  applies_to?: string[];
  rules: BackendPolicyRule[];
  enabled: boolean;
}

export type BackendPolicyRule =
  | { type: "max_cost_usd"; limit: number }
  | { type: "allowed_tools"; tools: string[] }
  | { type: "forbidden_tools"; tools: string[] }
  | { type: "require_approval"; tags: string[] }
  | { type: "block_pii" }
  | { type: "data_scope"; allowed_projects: string[] }
  | { type: "offline_only" };

export interface ComplianceReport {
  grade: string;
  score: number;
  checks: ComplianceCheck[];
}

export interface ComplianceCheck {
  name: string;
  passed: boolean;
  detail?: string;
}

// ═══════════════════════════════════════════════════════════════════════
// A2A / Federation types
// ═══════════════════════════════════════════════════════════════════════

export interface A2AAgentCard {
  name: string;
  version: string;
  description: string;
  url: string;
  provider?: { organization: string; url?: string };
  icon_url?: string;
  protocol_version: string;
  capabilities: { streaming: boolean; push_notifications: boolean; state_transition_history: boolean };
  authentication: { scheme: string }[];
  default_input_modes: string[];
  default_output_modes: string[];
  skills: A2ASkill[];
}

export interface A2ASkill {
  id: string;
  name: string;
  description: string;
  input_modes: string[];
  output_modes: string[];
  tags: string[];
  examples?: string[];
}

export interface A2APeer {
  name?: string;
  url: string;
  version?: string;
  description?: string;
  skills?: A2ASkill[];
  healthy?: boolean;
}

export interface FederationPeer {
  id: string;
  url: string;
  healthy: boolean;
  name?: string;
  last_seen?: string;
  skills?: string[];
}

// ═══════════════════════════════════════════════════════════════════════
// Collaboration types
// ═══════════════════════════════════════════════════════════════════════

export interface CollabSession {
  id: string;
  project_id: string;
  created_by: string;
  created_at: string;
}

export interface CollabParticipant {
  id: string;
  session_id: string;
  name: string;
  role: string;
  participant_type: string;
  joined_at: string;
}

export interface CollabActivity {
  id: string;
  session_id: string;
  participant_id: string;
  activity_type: string;
  target?: string;
  detail?: string;
  created_at: string;
}

// ═══════════════════════════════════════════════════════════════════════
// Self-Improvement types
// ═══════════════════════════════════════════════════════════════════════

export interface SelfImprovementReport {
  total_generations: number;
  avg_taste_score: number;
  build_success_rate: number;
  trend: string;
  weak_areas: string[];
}

export interface LearnedSkill {
  id: string;
  name: string;
  patterns: { pattern: string; weight: number }[];
  success_count: number;
  failure_count: number;
  status: string;
  tags: string[];
}

// ═══════════════════════════════════════════════════════════════════════
// Voice types
// ═══════════════════════════════════════════════════════════════════════

export interface VoiceChatResponse {
  transcript: string;
  response_text: string;
  response_audio_base64: string;
  response_mime_type: string;
  voice: string;
}

export class ApiError extends Error {
  status: number;
  constructor(status: number, message: string) {
    super(message);
    this.name = "ApiError";
    this.status = status;
  }
}

async function parseError(res: Response): Promise<ApiError> {
  let message = `Request failed (${res.status})`;
  try {
    const body = await res.text();
    const json = JSON.parse(body);
    // Backend envelope is `{ error: { code, message, hint } }`. Flattening to
    // the `message` field was producing "[object Object]" in toasts — extract
    // the string fields and combine.
    if (json?.error && typeof json.error === "object") {
      const err = json.error as { code?: string; message?: string; hint?: string };
      if (err.message) {
        message = err.hint ? `${err.message} — ${err.hint}` : err.message;
      } else if (err.code) {
        message = err.code;
      }
    } else if (typeof json?.error === "string") {
      message = json.error;
    } else if (body) {
      message = body;
    }
  } catch {
    // use default
  }
  return new ApiError(res.status, message);
}

async function get<T>(path: string): Promise<T> {
  const res = await fetch(`${BASE}${path}`);
  if (!res.ok) throw await parseError(res);
  return res.json();
}

async function post<T>(path: string, body?: unknown): Promise<T> {
  const res = await fetch(`${BASE}${path}`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: body !== undefined ? JSON.stringify(body) : undefined,
  });
  if (!res.ok) throw await parseError(res);
  return res.json();
}

async function put<T>(path: string, body?: unknown): Promise<T> {
  const res = await fetch(`${BASE}${path}`, {
    method: "PUT",
    headers: { "Content-Type": "application/json" },
    body: body !== undefined ? JSON.stringify(body) : undefined,
  });
  if (!res.ok) throw await parseError(res);
  return res.json();
}

async function del(path: string): Promise<void> {
  const res = await fetch(`${BASE}${path}`, { method: "DELETE" });
  if (!res.ok && res.status !== 204) throw await parseError(res);
}

export const api = {
  // Projects
  listProjects: () => get<Project[]>("/projects"),
  createProject: (name: string, description?: string) =>
    post<Project>("/projects", { name, description }),
  getProject: (id: string) => get<Project>(`/projects/${id}`),
  deleteProject: (id: string) => del(`/projects/${id}`),
  forkProject: (id: string, name?: string) =>
    post<Project>(`/projects/${id}/fork`, { name }),

  // Project-level LLM model
  getProjectModel: (projectId: string) =>
    get<{ provider: string | null; model: string | null }>(`/projects/${projectId}/model`),
  setProjectModel: (projectId: string, provider: string | null, model: string | null) =>
    post(`/projects/${projectId}/model`, { provider, model }),

  // Conversations
  listConversations: (projectId: string) =>
    get<Conversation[]>(`/projects/${projectId}/conversations`),
  createConversation: (projectId: string) =>
    post<Conversation>(`/projects/${projectId}/conversations`),
  listMessages: (projectId: string, convId: string) =>
    get<NexusMessage[]>(`/projects/${projectId}/conversations/${convId}/messages`),

  // Knowledge
  listKnowledge: (projectId: string) =>
    get<KnowledgeItem[]>(`/projects/${projectId}/knowledge`),
  addKnowledge: (projectId: string, item: Omit<KnowledgeItem, "id" | "project_id" | "created_at">) =>
    post<KnowledgeItem>(`/projects/${projectId}/knowledge`, item),
  deleteKnowledge: (projectId: string, itemId: string) =>
    del(`/projects/${projectId}/knowledge/${itemId}`),

  // Tables
  listTables: (projectId: string) =>
    get<MaterializedTable[]>(`/projects/${projectId}/tables`),
  listRecords: (projectId: string, tableName: string) =>
    get<{ columns: string[]; records: unknown[][] }>(`/projects/${projectId}/tables/${tableName}/records`),
  insertRecord: (projectId: string, tableName: string, record: Record<string, unknown>) =>
    post(`/projects/${projectId}/tables/${tableName}/records`, record),
  deleteRecord: (projectId: string, tableName: string, rowid: number) =>
    del(`/projects/${projectId}/tables/${tableName}/records/${rowid}`),

  // Agents
  listAgents: (projectId: string) =>
    get<AgentDefinition[]>(`/projects/${projectId}/agents`),
  createAgent: (projectId: string, agent: { name: string; role: string; tools: string[]; memory_type?: string; provider?: string; model?: string; system_prompt: string }) =>
    post<AgentDefinition>(`/projects/${projectId}/agents`, agent),
  designWorkflow: (projectId: string, idea: string, complexity?: string, industry?: string) =>
    post<{ success: boolean; workflow: DesignedWorkflow }>(`/projects/${projectId}/agents/design`, { idea, complexity, industry }),
  materializeWorkflow: (projectId: string, workflow: DesignedWorkflow) =>
    post<{ success: boolean; agents_created: number; agent_ids: string[] }>(`/projects/${projectId}/agents/design/materialize`, { workflow }),
  runAgent: (projectId: string, agentId: string) =>
    post(`/projects/${projectId}/agents/${agentId}/run`),
  stopAgent: (projectId: string, agentId: string) =>
    post(`/projects/${projectId}/agents/${agentId}/stop`),
  deployAgent: (projectId: string, agentId: string) =>
    post(`/projects/${projectId}/agents/${agentId}/deploy`),
  deleteAgent: (projectId: string, agentId: string) =>
    del(`/projects/${projectId}/agents/${agentId}`),
  updateAgentModel: (projectId: string, agentId: string, provider: string, model: string) =>
    put(`/projects/${projectId}/agents/${agentId}/model`, { provider, model }),

  // Agent Skills
  listBuiltinSkills: () =>
    get<AgentSkill[]>("/skills/builtins"),
  listSkills: (projectId: string) =>
    get<AgentSkill[]>(`/projects/${projectId}/skills`),
  createSkill: (projectId: string, skill: { name: string; description: string; icon?: string; category?: string; system_prompt: string; tools?: string[]; rules?: string[]; temperature?: number; max_tokens?: number }) =>
    post<AgentSkill>(`/projects/${projectId}/skills`, skill),
  updateSkill: (projectId: string, skillId: string, skill: { name: string; description: string; icon?: string; category?: string; system_prompt: string; tools?: string[]; rules?: string[]; temperature?: number; max_tokens?: number }) =>
    put<AgentSkill>(`/projects/${projectId}/skills/${skillId}`, skill),
  deleteSkill: (projectId: string, skillId: string) =>
    del(`/projects/${projectId}/skills/${skillId}`),
  listAgentSkills: (projectId: string, agentId: string) =>
    get<AgentSkill[]>(`/projects/${projectId}/agents/${agentId}/skills`),
  assignSkill: (projectId: string, agentId: string, skillId: string, priority?: number) =>
    post<SkillAssignment>(`/projects/${projectId}/agents/${agentId}/skills`, { skill_id: skillId, priority: priority ?? 0 }),
  unassignSkill: (projectId: string, agentId: string, skillId: string) =>
    del(`/projects/${projectId}/agents/${agentId}/skills/${skillId}`),
  previewAgentPrompt: (projectId: string, agentId: string) =>
    get<{ prompt: string; skills_used: string[] }>(`/projects/${projectId}/agents/${agentId}/prompt`),

  // Agent Loop (Claude Code-style)
  listAgentModels: () =>
    get<{ online: boolean; ollama_available: boolean; ollama_models: string[]; cloud_providers: string[] }>("/agent/models"),
  getProjectBrain: (projectId: string) =>
    get<{ scanned: boolean; brain?: { stack: { language: string; framework: string; package_manager: string; test_framework: string; has_typescript: boolean; has_docker: boolean; has_ci: boolean }; structure: string; patterns: string[]; decisions: { timestamp: string; description: string; context: string }[]; key_files: { path: string; role: string }[] } }>(`/projects/${projectId}/agent/brain`),
  rescanBrain: (projectId: string) =>
    post(`/projects/${projectId}/agent/brain/rescan`),

  // Workflows
  listWorkflowRuns: (projectId: string) =>
    get<WorkflowRun[]>(`/projects/${projectId}/workflows`),
  runWorkflow: (projectId: string, pipeline: string, context?: unknown) =>
    post(`/projects/${projectId}/workflows`, { pipeline, context }),

  // Portal
  getPortal: (projectId: string) =>
    get<Portal>(`/projects/${projectId}/portal`),
  publishPortal: (projectId: string, body: { project_name: string; agent_name: string; slug?: string }) =>
    post<Portal>(`/projects/${projectId}/portal/publish`, body),

  // App Runner
  startApp: (projectId: string, sandbox?: boolean) =>
    post<AppInstance>(`/projects/${projectId}/app/start`, sandbox !== undefined ? { sandbox } : undefined),
  stopApp: (projectId: string) =>
    post(`/projects/${projectId}/app/stop`),
  restartApp: (projectId: string) =>
    post<AppInstance>(`/projects/${projectId}/app/restart`),
  appStatus: (projectId: string) =>
    get<AppInstance | null>(`/projects/${projectId}/app/status`),
  listAppInstances: (projectId: string) =>
    get<AppInstance[]>(`/projects/${projectId}/app/instances`),
  appLogs: (projectId: string) =>
    get<{ instance_id: string; status: string; error?: string; logs: string }>(`/projects/${projectId}/app/logs`),
  getBuildSteps: (projectId: string) =>
    get<{ steps: BuildStep[] }>(`/projects/${projectId}/app/build-steps`),
  toggleAutoRestart: (projectId: string, enabled: boolean) =>
    post<{ auto_restart: boolean }>(`/projects/${projectId}/app/auto-restart`, { enabled }),
  listEnvVars: (projectId: string) =>
    get<EnvVar[]>(`/projects/${projectId}/app/env`),
  setEnvVar: (projectId: string, key: string, value: string) =>
    post<EnvVar>(`/projects/${projectId}/app/env`, { key, value }),
  deleteEnvVar: (projectId: string, key: string) =>
    del(`/projects/${projectId}/app/env/${encodeURIComponent(key)}`),

  // Files
  listFiles: (projectId: string) =>
    get<{ files: { path: string; type: string; name: string; size?: number; extension?: string }[]; root: string }>(`/projects/${projectId}/app/files`),
  readFile: (projectId: string, filePath: string) =>
    get<{ path: string; content: string; extension: string; size: number }>(`/projects/${projectId}/app/files/${encodeURIComponent(filePath)}`),
  writeFile: (projectId: string, filePath: string, content: string) =>
    put<{ path: string; size: number; status: string }>(`/projects/${projectId}/app/files/${encodeURIComponent(filePath)}`, { content }),
  deleteFile: (projectId: string, filePath: string) =>
    del(`/projects/${projectId}/app/files/${encodeURIComponent(filePath)}`),
  downloadZipUrl: (projectId: string) =>
    `${BASE}/projects/${projectId}/app/download`,
  pushToGithub: (projectId: string, body: { repo_url: string; branch?: string; message?: string; token?: string }) =>
    post<{ status: string; repo_url?: string; branch?: string; error?: string }>(`/projects/${projectId}/app/github`, body),
  deployToServer: (projectId: string, body: { target: string; ssh_key?: string; post_deploy_cmd?: string }) =>
    post<{ status: string; target?: string; error?: string }>(`/projects/${projectId}/app/deploy-server`, body),

  // Coding Tasks (parallel agent development)
  listCodingTasks: (projectId: string) =>
    get<CodingTask[]>(`/projects/${projectId}/coding/tasks`),
  getCodingTask: (projectId: string, taskId: string) =>
    get<CodingTask>(`/projects/${projectId}/coding/tasks/${taskId}`),
  createCodingTask: (projectId: string, task: { title: string; description: string; agent?: string; priority?: number }) =>
    post<CodingTask>(`/projects/${projectId}/coding/tasks`, task),
  createCodingBatch: (projectId: string, tasks: { title: string; description: string; agent?: string; priority?: number }[]) =>
    post<{ tasks: CodingTask[] }>(`/projects/${projectId}/coding/tasks/batch`, { tasks }),
  deleteCodingTask: (projectId: string, taskId: string) =>
    del(`/projects/${projectId}/coding/tasks/${taskId}`),

  // Code Generation
  codegenPlan: (projectId: string, ir: unknown) =>
    post(`/projects/${projectId}/codegen/plan`, { ir }),
  codegenGenerate: (projectId: string, ir: unknown) =>
    post<CodeGenResult>(`/projects/${projectId}/codegen/generate`, { ir }),

  // Planner
  generatePlan: (projectId: string, description: string) =>
    post<{ id: string; plan: unknown; estimated_files: number }>(`/projects/${projectId}/plan/generate`, { description }),
  approvePlan: (projectId: string, plan: unknown) =>
    post<{ status: string; codegen_result: unknown }>(`/projects/${projectId}/plan/approve`, { plan }),
  getPlan: (projectId: string) =>
    get<{ exists: boolean; plan?: unknown }>(`/projects/${projectId}/plan`),

  // Settings / LLM Providers
  listProviders: () =>
    get<{ id: string; name: string; api_base: string; requires_key: boolean; models: { id: string; name: string; context_window: number }[] }[]>("/settings/providers"),
  listApiKeys: () =>
    get<{ providers: { provider: string; configured: boolean; source: string }[] }>("/settings/api-keys"),
  setApiKey: (provider: string, apiKey: string) =>
    post("/settings/api-keys", { provider, api_key: apiKey }),
  deleteApiKey: (provider: string) =>
    del(`/settings/api-keys/${provider}`),
  getDefaultModel: () =>
    get<{ provider: string; model: string }>("/settings/default-model"),
  setDefaultModel: (provider: string, model: string) =>
    post("/settings/default-model", { provider, model }),

  // Sandbox settings
  getSandboxSettings: () =>
    get<SandboxSettings>("/settings/sandbox"),
  setSandboxSettings: (enabled: boolean) =>
    post<{ status: string; enabled: boolean }>("/settings/sandbox", { enabled }),

  // Vault
  listVault: (projectId: string) =>
    get<VaultEntry[]>(`/projects/${projectId}/vault`),
  setVault: (projectId: string, key: string, value: string) =>
    post(`/projects/${projectId}/vault`, { key, value }),
  deleteVault: (projectId: string, key: string) =>
    del(`/projects/${projectId}/vault/${key}`),

  // HITL Gates
  listGates: (projectId: string) =>
    get<{ id: string; gate_type: string; status: string; description?: string; context?: Record<string, unknown>; created_at: string }[]>(`/projects/${projectId}/gates`),
  listPendingGates: (projectId: string) =>
    get<{ id: string; gate_type: string; status: string; description?: string; context?: Record<string, unknown>; created_at: string }[]>(`/projects/${projectId}/gates/pending`),
  resolveGate: (projectId: string, gateId: string, approved: boolean, reason?: string) =>
    post(`/projects/${projectId}/gates/resolve`, { gate_id: gateId, approved, reason }),

  // Cost Intelligence
  getCostSummary: () =>
    get<{ total_cost_usd: number; by_model: { model: string; cost_usd: number; tokens: number }[]; today_cost_usd: number }>("/costs/summary"),
  getCostBudget: () =>
    get<{ daily_limit_usd: number; used_today_usd: number; remaining_usd: number; percentage_used: number }>("/costs/budget"),
  getCostRecommendations: () =>
    get<{ recommendations: { type: string; description: string; savings_usd?: number }[] }>("/costs/recommendations"),

  // Code Graph
  getCodeGraph: (projectId: string) =>
    get<{ nodes: { id: string; label: string; type: string; file?: string }[]; edges: { from: string; to: string; type: string }[] }>(`/projects/${projectId}/code-graph`),
  rebuildCodeGraph: (projectId: string) =>
    post(`/projects/${projectId}/code-graph/rebuild`),

  // Invariants
  checkInvariants: (projectId: string) =>
    get<{ violations: { rule: string; severity: string; message: string; file?: string }[]; passed: number; failed: number }>(`/projects/${projectId}/invariants`),

  // Perceived Speed
  getSpeedEstimate: (projectId: string) =>
    get<{ estimated_seconds: number; steps: { name: string; estimated_ms: number }[] }>(`/projects/${projectId}/speed/estimate`),

  // Background Agents
  listBackgroundAgents: (projectId: string) =>
    get<{ id: string; name: string; task: string; status: string; interval_seconds?: number }[]>(`/projects/${projectId}/background-agents`),

  // Explain My App
  explainApp: (projectId: string) =>
    get<{ explanation: string; architecture: string; features: string[] }>(`/projects/${projectId}/explain`),

  // Enforcement
  enforceGate: (projectId: string, gate?: string) =>
    get<Record<string, unknown>>(`/projects/${projectId}/enforce${gate ? `?gate=${gate}` : ""}`),

  // Observability
  listTraces: (projectId: string) =>
    get<{ traces: unknown[] }>(`/projects/${projectId}/traces`),
  listHealing: (projectId: string) =>
    get<{ events: unknown[] }>(`/projects/${projectId}/healing`),

  // ═══════════════════════════════════════════════════════════════════════
  // Intelligence Layer (Nexus Brain + Learning + Control)
  // ═══════════════════════════════════════════════════════════════════════

  // Brain Analysis
  brainAnalyze: (description: string) =>
    post<BrainOutput>("/brain/analyze", { description }),
  brainDesignAgents: (description: string) =>
    post<{ agents: DesignedAgent[]; count: number }>("/brain/agents", { description }),
  brainHiddenRequirements: (description: string) =>
    post<{ intent: unknown; hidden_requirements: HiddenRequirements }>("/brain/hidden-requirements", { description }),

  // Taste Gate (Quality)
  runTasteGate: (projectId: string) =>
    post<TasteGateResult>(`/projects/${projectId}/taste-gate`),

  // Autonomous Engine
  runAutonomous: (projectId: string, maxIterations?: number) =>
    post<AutonomousResult>(`/projects/${projectId}/autonomous`, { max_iterations: maxIterations ?? 5 }),

  // Predictions
  predict: (recentMessages?: string[], projectId?: string) =>
    post<Prediction>("/living/predict", { recent_messages: recentMessages, project_id: projectId }),

  // Model Routing
  routeModel: (taskType: string, complexity?: string, isRetry?: boolean) =>
    post<ModelSelection>("/living/route-model", { task_type: taskType, complexity, is_retry: isRetry }),

  // Control Plane
  setControlMode: (mode: "safe" | "assisted" | "autonomous", limits?: Record<string, unknown>) =>
    post<{ mode: string; limits: unknown; applied: boolean }>("/control/mode", { mode, limits }),
  getControlStatus: () =>
    get<{ mode: string; limits: unknown; capabilities: Record<string, string> }>("/control/status"),

  // Feedback
  submitFeedback: (feedbackType: "positive" | "negative" | "suggestion", context: string, message: string, target?: string, projectId?: string) =>
    post<{ updates_applied: string[]; future_impact: string }>("/feedback", {
      feedback_type: feedbackType, context, message, target, project_id: projectId,
    }),

  // Runtime Testing
  runTests: (projectId: string) =>
    post<TestResult>(`/projects/${projectId}/test`),

  // Unified Memory
  getUnifiedMemory: () =>
    get<{ memory: UnifiedMemory; context_preview: string }>("/memory/unified"),
  getProjectMemory: (projectId: string) =>
    get<{ memory: UnifiedMemory; context_preview: string }>(`/projects/${projectId}/memory`),

  // Strategy
  getStrategy: (projectId: string, description: string) =>
    post<StrategyPlan>(`/projects/${projectId}/strategy`, { description }),
  getNextFeature: (projectId: string, description: string) =>
    post<{ next_feature: FeatureSuggestion | null }>(`/projects/${projectId}/strategy/next`, { description }),

  // Causal Learning
  getCausalGraph: () =>
    get<CausalGraph>("/causal/graph"),

  // Agent Council
  councilDeliberate: (description: string) =>
    post<CollectiveDecision>("/council/deliberate", { description }),

  // Anticipation
  anticipate: (recentMessages?: string[], actionHistory?: { action_type: string; target: string; timestamp_ms: number }[]) =>
    post<AnticipationReport>("/anticipate", { recent_messages: recentMessages, action_history: actionHistory }),

  // Project Intelligence
  getProjectIntelligence: (projectId: string) =>
    get<{ intelligence: ProjectIntelligence; context_preview: string }>(`/projects/${projectId}/intelligence`),
  improveProject: (projectId: string) =>
    post<{ analysis: unknown; improvement: unknown }>(`/projects/${projectId}/improve`),

  // Runtime Health
  getRuntimeHealth: (projectId: string) =>
    get<RuntimeHealthReport>(`/projects/${projectId}/health`),

  // Runtime Observer
  recordRuntimeEvent: (projectId: string, eventType: string, severity?: number, metadata?: unknown) =>
    post(`/projects/${projectId}/observe`, { event_type: eventType, severity, metadata }),

  // Prompt Evolution
  getPromptPerformance: () =>
    get<{ prompts: PromptPerformance[] }>("/prompts/performance"),

  // Business (CEO Dashboard)
  getBusinessOverview: (projectId: string) =>
    get<Record<string, unknown>>(`/projects/${projectId}/business`),
  listBusinessTeams: (projectId: string) =>
    get<Record<string, unknown>>(`/projects/${projectId}/business/teams`),
  createBusinessTeam: (projectId: string, templateName: string, autonomyLevel?: string) =>
    post<Record<string, unknown>>(`/projects/${projectId}/business/teams`, {
      template_name: templateName,
      autonomy_level: autonomyLevel ?? "supervised",
    }),
  listBusinessEvents: (projectId: string) =>
    get<Record<string, unknown>>(`/projects/${projectId}/business/events`),

  // Kernel Processes
  listProcesses: async (): Promise<KernelProcess[]> => {
    const data = await get<{ processes: KernelProcess[] } | KernelProcess[]>(
      "/kernel/processes",
    );
    if (Array.isArray(data)) return data;
    return data.processes ?? [];
  },
  suspendProcess: (pid: string) =>
    post<{ status: string }>(`/kernel/processes/${encodeURIComponent(pid)}/suspend`),
  resumeProcess: (pid: string) =>
    post<{ status: string }>(`/kernel/processes/${encodeURIComponent(pid)}/resume`),
  killProcess: (pid: string) =>
    post<{ status: string }>(`/kernel/processes/${encodeURIComponent(pid)}/kill`),

  // Memory — project-scoped episodes, filtered client-side.
  // Backend exposes embedding-based recall; the UI uses text search over the
  // episode list so users don't need to generate embeddings themselves.
  recallMemory: async (projectId: string, query: string): Promise<MemoryEntry[]> => {
    const data = await get<{ episodes: Array<{
      id: string;
      summary?: string;
      tags?: string[];
      timestamp?: string;
      importance?: number;
    }> }>(`/memory/episodes/project/${projectId}`);
    const episodes = data.episodes ?? [];
    const q = query.trim().toLowerCase();
    const filtered = q
      ? episodes.filter(
          (ep) =>
            (ep.summary ?? "").toLowerCase().includes(q) ||
            (ep.tags ?? []).some((t) => t.toLowerCase().includes(q)),
        )
      : episodes;
    return filtered.map((ep) => ({
      id: ep.id,
      content: ep.summary ?? "(no summary)",
      tags: ep.tags ?? [],
      timestamp: ep.timestamp ?? new Date().toISOString(),
      relevance: ep.importance,
    }));
  },
  storeMemory: async (
    _projectId: string,
    _content: string,
    _tags: string[],
  ): Promise<MemoryEntry> => {
    throw new Error(
      "Memory writes happen automatically as agents run. Manual storage is not available yet.",
    );
  },

  // Credits
  getCredits: () =>
    get<{ credits_total: number; credits_used: number; credits_remaining: number; plan: string; resets_at: string }>("/credits"),

  // User Preferences
  getPreferences: () =>
    get<{ display_mode: string; onboarding_completed: boolean; onboarding_step: number; preferred_quality: string }>("/preferences"),
  updatePreferences: (prefs: { display_mode?: string; preferred_quality?: string }) =>
    put<void>("/preferences", prefs),
  completeOnboardingStep: (step: number) =>
    post<void>("/preferences/onboarding", { step }),

  // Templates
  listTemplates: () =>
    get<{ templates: Array<{ id: string; name: string; description: string; category: string; icon: string; preview_prompt: string; features: string[]; estimated_credits: number }> }>("/templates"),

  // ═══════════════════════════════════════════════════════════════════════
  // Admin / System
  // ═══════════════════════════════════════════════════════════════════════

  getHealth: () =>
    get<Record<string, unknown>>("/health/detailed"),
  tripBreaker: () =>
    post<{ status: string }>("/control/breaker/trip"),
  resetBreaker: () =>
    post<{ status: string }>("/control/breaker/reset"),
  getAuditLog: () =>
    get<{ entries: unknown[] }>("/audit/log"),
  listAuthKeys: () =>
    get<{ keys: { id: string; name: string; prefix: string; created_at: string }[] }>("/auth/keys"),
  createAuthKey: (body: { name: string; scopes?: string[] }) =>
    post<{ id: string; key: string; name: string; created_at: string }>("/auth/keys", body),
  revokeAuthKey: (id: string) =>
    del(`/auth/keys/${id}`),

  // ═══════════════════════════════════════════════════════════════════════
  // Observability
  // ═══════════════════════════════════════════════════════════════════════

  getTraces: () =>
    get<{ traces: unknown[] }>("/observability/traces"),
  getTrace: (traceId: string) =>
    get<Record<string, unknown>>(`/observability/traces/${traceId}`),
  setCostBudget: (budget: { daily_limit_usd: number }) =>
    post<{ daily_limit_usd: number; applied: boolean }>("/costs/budget", budget),

  // ═══════════════════════════════════════════════════════════════════════
  // Super Agents
  // ═══════════════════════════════════════════════════════════════════════

  getSuperAgentStatus: () =>
    get<Record<string, unknown>>("/super-agents/status"),
  listSuperAgents: () =>
    get<{ agents: { id: string; name: string; status: string; last_run?: string }[] }>("/super-agents/agents"),
  getSuperAgentMetrics: () =>
    get<Record<string, unknown>>("/super-agents/metrics"),
  triggerSuperAgent: (agentId: string) =>
    post<{ status: string }>(`/super-agents/trigger`, { agent_id: agentId }),
  pauseSuperAgents: () =>
    post<{ status: string }>("/super-agents/pause"),

  // ═══════════════════════════════════════════════════════════════════════
  // Eval
  // ═══════════════════════════════════════════════════════════════════════

  listEvalSuites: () =>
    get<{ suites: { id: string; name: string; description: string; test_count: number }[] }>("/eval/suites"),
  runEval: (suiteId: string) =>
    post<{ run_id: string; status: string }>("/eval/run", { suite_id: suiteId }),
  getEvalResults: () =>
    get<{ results: { suite_id: string; passed: number; failed: number; score: number; ran_at: string }[] }>("/eval/results"),

  // ═══════════════════════════════════════════════════════════════════════
  // MCP Servers
  // ═══════════════════════════════════════════════════════════════════════

  listMcpServers: () =>
    get<{ servers: { id: string; name: string; url: string; status: string }[] }>("/mcp/servers"),
  addMcpServer: (body: { name: string; url: string; api_key?: string }) =>
    post<{ id: string; name: string; url: string; status: string }>("/mcp/servers", body),
  removeMcpServer: (id: string) =>
    del(`/mcp/servers/${id}`),
  connectMcpServer: (id: string) =>
    post<{ status: string }>(`/mcp/servers/${id}/connect`),
  getMcpServerTools: (id: string) =>
    get<{ tools: { name: string; description: string; parameters: unknown }[] }>(`/mcp/servers/${id}/tools`),

  // ═══════════════════════════════════════════════════════════════════════
  // Webhooks
  // ═══════════════════════════════════════════════════════════════════════

  listWebhooks: () =>
    get<{ webhooks: { id: string; url: string; events: string[]; active: boolean; created_at: string }[] }>("/webhooks"),
  createWebhook: (body: { url: string; events: string[]; secret?: string }) =>
    post<{ id: string; url: string; events: string[]; active: boolean; created_at: string }>("/webhooks", body),
  deleteWebhook: (id: string) =>
    del(`/webhooks/${id}`),
  testWebhook: (id: string) =>
    post<{ status: string; response_code?: number }>(`/webhooks/${id}/test`),

  // ═══════════════════════════════════════════════════════════════════════
  // Federation
  // ═══════════════════════════════════════════════════════════════════════

  listKernelFederationPeers: () =>
    get<{ peers: { id: string; name: string; url: string; status: string; connected_at?: string }[] }>("/kernel/federation/peers"),
  connectPeer: (body: { url: string; name?: string; token?: string }) =>
    post<{ peer_id: string; status: string }>("/kernel/federation/connect", body),
  disconnectPeer: (peerId: string) =>
    del(`/kernel/federation/${peerId}`),

  // ═══════════════════════════════════════════════════════════════════════
  // Workflows (global)
  // ═══════════════════════════════════════════════════════════════════════

  listWorkflows: () =>
    get<{ workflows: { id: string; name: string; status: string; created_at: string }[] }>("/workflows"),
  startWorkflow: (body: { name: string; input?: Record<string, unknown> }) =>
    post<{ id: string; status: string }>("/workflows/start", body),
  getWorkflow: (id: string) =>
    get<{ id: string; name: string; status: string; steps: unknown[]; created_at: string }>(`/workflows/${id}`),
  approveWorkflowStep: (id: string, step: string) =>
    post<{ status: string }>(`/workflows/${id}/approve/${step}`),
  cancelWorkflow: (id: string) =>
    post<{ status: string }>(`/workflows/${id}/cancel`),
  resumeWorkflow: (id: string) =>
    post<{ status: string }>(`/workflows/${id}/resume`),

  // ═══════════════════════════════════════════════════════════════════════
  // Plugins
  // ═══════════════════════════════════════════════════════════════════════

  listPlugins: () =>
    get<{ plugins: { id: string; name: string; version: string; enabled: boolean; description: string }[] }>("/plugins/all"),
  installPlugin: (body: { name: string; version?: string; source?: string }) =>
    post<{ id: string; name: string; version: string; status: string }>("/plugins/install", body),
  enablePlugin: (id: string) =>
    post<{ status: string }>(`/plugins/${id}/enable`),
  disablePlugin: (id: string) =>
    post<{ status: string }>(`/plugins/${id}/disable`),

  // ═══════════════════════════════════════════════════════════════════════
  // Quality / Taste
  // ═══════════════════════════════════════════════════════════════════════

  runTasteScore: (projectId: string) =>
    post<{ score: number; breakdown: Record<string, number> }>(`/projects/${projectId}/taste/score`),
  getTasteReport: (projectId: string) =>
    get<{ score: number; breakdown: Record<string, number>; issues: { area: string; description: string; severity: string }[] }>(`/projects/${projectId}/taste/report`),
  getTasteHistory: (projectId: string) =>
    get<{ history: { score: number; timestamp: string }[] }>(`/projects/${projectId}/taste/history`),
  triggerRedesign: (projectId: string) =>
    post<{ status: string; mutations_applied: number; score_before: number; score_after: number }>(`/projects/${projectId}/taste/redesign`),

  // ═══════════════════════════════════════════════════════════════════════
  // Gates / Approvals (extended)
  // ═══════════════════════════════════════════════════════════════════════

  getPendingGates: (projectId: string) =>
    get<{ gates: { id: string; gate_type: string; status: string; description?: string; context?: Record<string, unknown>; created_at: string }[] }>(`/projects/${projectId}/gates/pending`),

  // ═══════════════════════════════════════════════════════════════════════
  // Vault (extended key-based access)
  // ═══════════════════════════════════════════════════════════════════════

  getVaultSecret: (projectId: string, key: string) =>
    get<{ key: string; value: string }>(`/projects/${projectId}/vault/${encodeURIComponent(key)}`),
  setVaultSecret: (projectId: string, key: string, value: string) =>
    post<{ key: string; status: string }>(`/projects/${projectId}/vault/${encodeURIComponent(key)}`, { value }),
  deleteVaultSecret: (projectId: string, key: string) =>
    del(`/projects/${projectId}/vault/${encodeURIComponent(key)}`),

  // ═══════════════════════════════════════════════════════════════════════
  // Memory (extended)
  // ═══════════════════════════════════════════════════════════════════════

  getMemoryEpisodes: (projectId: string) =>
    get<{ episodes: { id: string; content: string; tags: string[]; timestamp: string }[] }>(`/memory/episodes/project/${projectId}`),
  getMemoryKnowledge: () =>
    get<{ items: { key: string; value: string; category: string; confidence: number }[] }>("/memory/knowledge"),
  getMemoryHealth: () =>
    get<{ status: string; total_entries: number; oldest_entry?: string; newest_entry?: string }>("/memory/health"),

  // ═══════════════════════════════════════════════════════════════════════
  // Governance
  // ═══════════════════════════════════════════════════════════════════════

  listPolicies: () =>
    get<{ policies: GovernancePolicy[]; halted: boolean }>("/governance/policies"),
  upsertPolicy: (policy: GovernancePolicy) =>
    post<{ id: string; status: string }>("/governance/policies", policy),
  deletePolicy: (id: string) =>
    del(`/governance/policies/${encodeURIComponent(id)}`),
  evaluateAction: (req: { agent_id: string; tool_name?: string; estimated_cost_usd?: number; action_tags?: string[] }) =>
    post<{ decision: string; reason?: string }>("/governance/evaluate", req),
  killSwitch: (active: boolean, reason?: string) =>
    post<{ halted: boolean }>("/governance/kill-switch", { active, reason }),
  checkCompliance: (req: { action: string; tool_name?: string; data_contains_pii?: boolean; has_human_approval?: boolean }) =>
    post<ComplianceReport>("/governance/compliance", req),
  redactPii: (text: string) =>
    post<{ redacted: string; detected_pii_types: string[]; pii_found: boolean }>("/governance/pii/redact", { text }),

  // ═══════════════════════════════════════════════════════════════════════
  // A2A Federation
  // ═══════════════════════════════════════════════════════════════════════

  getAgentCard: () =>
    get<A2AAgentCard>("/.well-known/agent.json"),
  listA2APeers: () =>
    get<{ peers: A2APeer[] }>("/a2a/peers"),
  discoverPeer: (baseUrl: string) =>
    post<A2APeer>("/a2a/peers", { base_url: baseUrl }),
  removePeer: (urlEncoded: string) =>
    del(`/a2a/peers/${encodeURIComponent(urlEncoded)}`),
  listFederationPeers: () =>
    get<{ peers: FederationPeer[]; count: number }>("/a2a/federation/peers"),
  registerFederationPeer: (url: string) =>
    post<FederationPeer>("/a2a/federation/peers", { url }),
  removeFederationPeer: (id: string) =>
    del(`/a2a/federation/peers/${encodeURIComponent(id)}`),
  delegateToFederation: (message: string, requiredSkill?: string) =>
    post<Record<string, unknown>>("/a2a/federation/delegate", { message, required_skill: requiredSkill }),
  federationHealthCheck: () =>
    post<{ total: number; healthy: number; unhealthy: number }>("/a2a/federation/health-check", {}),

  // ═══════════════════════════════════════════════════════════════════════
  // Collaboration / Presence
  // ═══════════════════════════════════════════════════════════════════════

  createCollabSession: (projectId: string) =>
    post<CollabSession>(`/projects/${projectId}/collab`, {}),
  joinCollabSession: (projectId: string, sessionId: string, name: string, role: string) =>
    post<CollabParticipant>(`/projects/${projectId}/collab/${sessionId}/join`, { name, role }),
  listParticipants: (projectId: string, sessionId: string) =>
    get<CollabParticipant[]>(`/projects/${projectId}/collab/${sessionId}/participants`),
  postActivity: (projectId: string, sessionId: string, activity: { participant_id: string; activity_type: string; target?: string; detail?: string }) =>
    post<CollabActivity>(`/projects/${projectId}/collab/${sessionId}/activity`, activity),
  recentActivity: (projectId: string, sessionId: string) =>
    get<CollabActivity[]>(`/projects/${projectId}/collab/${sessionId}/activity`),

  // ═══════════════════════════════════════════════════════════════════════
  // Self-Improvement / Learning
  // ═══════════════════════════════════════════════════════════════════════

  selfImprovementLearn: (req: { project_id: string; description: string; taste_score: number; build_success: boolean; duration_ms: number }) =>
    post<{ episodes_recorded: number; signals_recorded: number; patterns_learned: number }>("/self-improvement/learn", req),
  selfImprovementReport: () =>
    get<SelfImprovementReport>("/self-improvement/report"),
  selfImprovementSkills: () =>
    get<{ skills: LearnedSkill[]; count: number }>("/self-improvement/skills"),
  selfImprovementExtract: (req: { project_id: string; description: string; quality_score: number }) =>
    post<{ extracted: boolean; skill_id?: string; name?: string; patterns?: number }>("/self-improvement/extract", req),
  selfImprovementPromote: () =>
    post<{ promoted: number; message: string }>("/self-improvement/promote", {}),
  selfImprovementRecordUsage: (skillId: string, success: boolean) =>
    post<{ recorded: boolean }>("/self-improvement/skills/usage", { skill_id: skillId, success }),

  // ═══════════════════════════════════════════════════════════════════════
  // Voice / TTS
  // ═══════════════════════════════════════════════════════════════════════

  textToSpeech: (text: string, voice?: string, model?: string) =>
    post<{ audio_base64: string; mime_type: string; voice: string }>("/multimodal/tts", { text, voice, model }),
  voiceChat: (req: {
    audio_base64: string;
    mime_type?: string;
    system_prompt?: string;
    voice?: string;
    messages?: { role: string; content: string }[];
  }) =>
    post<VoiceChatResponse>("/multimodal/voice-chat", req),

  // ═══════════════════════════════════════════════════════════════════════
  // Deploy
  // ═══════════════════════════════════════════════════════════════════════

  deployProject: (projectId: string, target: string, opts?: Record<string, unknown>) =>
    post<{ status: string; url?: string; logs?: string }>(`/projects/${projectId}/deploy`, { target, ...opts }),

  // ═══════════════════════════════════════════════════════════════════════
  // Mutation engine — incremental AI-driven code edits ("Fix with AI")
  // ═══════════════════════════════════════════════════════════════════════
  mutateProject: (
    projectId: string,
    change: string,
    targetFile?: string,
  ) =>
    post<{
      files_changed: string[];
      validation?: { ok: boolean; errors: string[] };
      applied: boolean;
      duration_ms: number;
    }>(`/projects/${projectId}/mutate`, { change, target_file: targetFile }),
};

// SSE chat — uses fetch + ReadableStream (POST not supported by EventSource)
export interface ChatEvent {
  type: "token" | "action_result" | "done" | "error";
  content?: string;
  action?: string;
  success?: boolean;
  data?: Record<string, unknown>;
  message?: string;
  conversation_id?: string;
  nexus_state?: {
    phase: number;
    phase_label: string;
    new_items: unknown[];
    actions: unknown[];
    milestone?: string;
  };
}

export async function streamChat(
  projectId: string,
  message: string,
  conversationId: string | undefined,
  onEvent: (event: ChatEvent) => void,
  signal?: AbortSignal,
  provider?: string,
  model?: string,
): Promise<void> {
  const res = await fetch(`${BASE}/projects/${projectId}/chat`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ message, conversation_id: conversationId, provider, model }),
    signal,
  });

  if (!res.ok) throw new Error(await res.text());

  const reader = res.body!.getReader();
  const decoder = new TextDecoder();
  let buffer = "";

  while (true) {
    const { done, value } = await reader.read();
    if (done) break;

    buffer += decoder.decode(value, { stream: true });

    const lines = buffer.split("\n");
    buffer = lines.pop() ?? "";

    for (const line of lines) {
      const trimmed = line.trim();
      if (!trimmed || trimmed.startsWith(":")) continue;

      if (trimmed.startsWith("data: ")) {
        const raw = trimmed.slice(6);
        try {
          const event = JSON.parse(raw) as ChatEvent;
          onEvent(event);
        } catch {
          // ignore malformed
        }
      }
    }
  }
}

// ═══════════════════════════════════════════════════════════════════════════
// Oneshot SSE Stream — full app generation with live thinking
// ═══════════════════════════════════════════════════════════════════════════

export interface OneShotEvent {
  type: string;
  // Phase events
  phase?: string;
  status?: string;
  detail?: string;
  // Progress
  percent?: number;
  message?: string;
  // Intent
  app_type?: string;
  complexity?: string;
  domain?: string;
  needs_auth?: boolean;
  needs_database?: boolean;
  // Decisions
  frontend?: string;
  database?: string;
  auth?: string;
  learning_overrides?: string[];
  // Product
  hero_headline?: string;
  personas?: number;
  features?: number;
  // Thinking
  icon?: string;
  progress?: number;
  // Explanation
  decision?: string;
  reason?: string;
  confidence?: number;
  alternatives?: string[];
  // Files
  path?: string;
  content?: string;
  lines?: number;
  skeleton_type?: string;
  // Estimate
  total_estimated_ms?: number;
  // Taste
  overall?: number;
  redesign_triggered?: boolean;
  mutations_applied?: number;
  score_before?: number;
  score_after?: number;
  // Complete
  project_id?: string;
  project_name?: string;
  taste_score?: number;
  files_count?: number;
  duration_ms?: number;
  app_url?: string | null;
  // Error
  fatal?: boolean;
  // Heartbeat
  elapsed_ms?: number;
  count?: number;
}

/**
 * Stream a oneshot app generation. Emits typed events as the pipeline progresses.
 * The backend streams SSE events covering: intent analysis, decisions, codegen,
 * taste scoring, redesign, and completion.
 */
async function consumeOneshotSSE(
  res: Response,
  onEvent: (event: OneShotEvent) => void,
): Promise<void> {
  if (!res.ok) throw new Error(await res.text());

  const reader = res.body!.getReader();
  const decoder = new TextDecoder();
  let buffer = "";

  while (true) {
    const { done, value } = await reader.read();
    if (done) break;

    buffer += decoder.decode(value, { stream: true });

    const lines = buffer.split("\n");
    buffer = lines.pop() ?? "";

    for (const line of lines) {
      const trimmed = line.trim();
      if (!trimmed || trimmed.startsWith(":")) continue;

      if (trimmed.startsWith("data: ")) {
        const raw = trimmed.slice(6);
        try {
          const event = JSON.parse(raw) as OneShotEvent;
          onEvent(event);
        } catch {
          // ignore malformed SSE
        }
      }
    }
  }
}

export async function streamOneshot(
  description: string,
  onEvent: (event: OneShotEvent) => void,
  signal?: AbortSignal,
  options?: { auto_redesign?: boolean; taste_threshold?: number },
): Promise<void> {
  const res = await fetch(`${BASE}/oneshot`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({
      description,
      auto_redesign: options?.auto_redesign ?? true,
      taste_threshold: options?.taste_threshold ?? 70,
      stream: true,
    }),
    signal,
  });
  return consumeOneshotSSE(res, onEvent);
}

/**
 * Stream a oneshot generation against an already-created project.
 * Use this after calling api.createProject so the UI can navigate to the
 * workspace immediately and stream progress from there.
 */
export async function streamOneshotForProject(
  projectId: string,
  description: string,
  onEvent: (event: OneShotEvent) => void,
  signal?: AbortSignal,
  options?: { auto_redesign?: boolean; taste_threshold?: number },
): Promise<void> {
  const res = await fetch(`${BASE}/projects/${projectId}/oneshot/start`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({
      description,
      auto_redesign: options?.auto_redesign ?? true,
      taste_threshold: options?.taste_threshold ?? 70,
    }),
    signal,
  });
  return consumeOneshotSSE(res, onEvent);
}
