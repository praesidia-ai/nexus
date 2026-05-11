import { BASE } from "./api";

// ---------------------------------------------------------------------------
// Team Types
// ---------------------------------------------------------------------------

export interface TeamTemplate {
  id: string;
  name: string;
  description: string;
  category: string;
  protocol: "sequential" | "parallel" | "debate" | "hierarchical" | "swarm";
  members: TeamMemberTemplate[];
  estimatedCostPerRun: number;
  tags: string[];
  icon?: string;
}

export interface TeamMemberTemplate {
  role: string;
  name: string;
  model: string;
  tools: string[];
  systemPrompt?: string;
}

export interface TeamConfig {
  name: string;
  description?: string;
  templateId?: string;
  protocol: string;
  members: TeamMemberConfig[];
  budget: number;
  maxDuration?: number;
}

export interface TeamMemberConfig {
  name: string;
  role: string;
  model: string;
  tools: string[];
  systemPrompt?: string;
}

export interface Team {
  id: string;
  name: string;
  description?: string;
  protocol: string;
  members: TeamMemberConfig[];
  budget: number;
  createdAt: string;
}

export interface TeamRun {
  id: string;
  teamId: string;
  task: string;
  status: "idle" | "running" | "paused" | "completed" | "failed";
  startedAt: string;
  completedAt?: string;
  duration: number;
  cost: number;
  artifactCount: number;
  messageCount: number;
}

export interface TeamRunResult {
  runId: string;
  teamId: string;
  task: string;
  status: string;
  duration: number;
  totalCost: number;
  costBreakdown: { member: string; cost: number; tokens: number }[];
  artifacts: TeamRunArtifact[];
  messages: TeamRunMessage[];
  summary: string;
}

export interface TeamRunArtifact {
  id: string;
  name: string;
  type: string;
  content: string;
  createdBy: string;
  createdAt: string;
  size: number;
}

export interface TeamRunMessage {
  id: string;
  timestamp: string;
  from: string;
  to: string;
  type: string;
  content: string;
}

// ---------------------------------------------------------------------------
// Marketplace Types
// ---------------------------------------------------------------------------

export interface MarketplaceContribution {
  id: string;
  name: string;
  description: string;
  author: string;
  category: "agent" | "team" | "plugin" | "domain_pack";
  tags: string[];
  version: string;
  downloads: number;
  rating: number;
  installed: boolean;
  icon?: string;
  createdAt: string;
}

// ---------------------------------------------------------------------------
// API
// ---------------------------------------------------------------------------

async function get<T>(path: string): Promise<T> {
  const res = await fetch(`${BASE}${path}`);
  if (!res.ok) {
    const text = await res.text().catch(() => "Request failed");
    throw new Error(text);
  }
  return res.json();
}

async function post<T>(path: string, body?: unknown): Promise<T> {
  const res = await fetch(`${BASE}${path}`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: body !== undefined ? JSON.stringify(body) : undefined,
  });
  if (!res.ok) {
    const text = await res.text().catch(() => "Request failed");
    throw new Error(text);
  }
  return res.json();
}

async function del(path: string): Promise<void> {
  const res = await fetch(`${BASE}${path}`, { method: "DELETE" });
  if (!res.ok && res.status !== 204) {
    const text = await res.text().catch(() => "Request failed");
    throw new Error(text);
  }
}

// ---------------------------------------------------------------------------
// Template normalization (backend may nest under `template`, different field names)
// ---------------------------------------------------------------------------

function modelFromPreference(pref: unknown): string {
  if (pref && typeof pref === "object" && pref !== null && "model" in pref) {
    return String((pref as { model: unknown }).model);
  }
  return "claude-sonnet-4-6";
}

function teamRoleToString(role: unknown): string {
  if (typeof role === "string") return role;
  if (!role || typeof role !== "object") return "Member";
  const r = role as Record<string, unknown>;
  if ("Lead" in r) return "Lead";
  if ("Reviewer" in r) return "Reviewer";
  if ("Coordinator" in r) return "Coordinator";
  if ("Observer" in r) return "Observer";
  if ("Specialist" in r && r.Specialist && typeof r.Specialist === "object") {
    const d = (r.Specialist as { domain?: string }).domain;
    return d ? `Specialist (${d})` : "Specialist";
  }
  if ("Custom" in r && r.Custom && typeof r.Custom === "object") {
    const n = (r.Custom as { name?: string }).name;
    return n ?? "Custom";
  }
  return "Member";
}

function coordinationToProtocol(coord: unknown): TeamTemplate["protocol"] {
  if (!coord || typeof coord !== "object") return "sequential";
  const protocol = (coord as { protocol?: string }).protocol;
  switch (protocol) {
    case "hierarchical":
      return "hierarchical";
    case "parallel":
      return "parallel";
    case "sequential":
      return "sequential";
    case "consensus":
    case "competitive":
      return "debate";
    case "swarm":
      return "swarm";
    default:
      return "sequential";
  }
}

function normalizeMember(m: unknown): TeamMemberTemplate {
  if (!m || typeof m !== "object") {
    return { role: "Member", name: "", model: "claude-sonnet-4-6", tools: [] };
  }
  const o = m as Record<string, unknown>;
  const agent =
    o.agent && typeof o.agent === "object" ? (o.agent as Record<string, unknown>) : undefined;

  if (agent) {
    const name = typeof agent.name === "string" ? agent.name : String(o.id ?? "Agent");
    const tools = Array.isArray(agent.tools) ? agent.tools.map(String) : [];
    return {
      role: teamRoleToString(o.role),
      name,
      model: modelFromPreference(agent.model_preference),
      tools,
    };
  }

  return {
    role: typeof o.role === "string" ? o.role : teamRoleToString(o.role),
    name: String(o.name ?? ""),
    model: typeof o.model === "string" ? o.model : modelFromPreference(o.model_preference),
    tools: Array.isArray(o.tools) ? o.tools.map(String) : [],
  };
}

const FRONTEND_PROTOCOLS: TeamTemplate["protocol"][] = [
  "sequential",
  "parallel",
  "debate",
  "hierarchical",
  "swarm",
];

function coerceProtocol(p: unknown): TeamTemplate["protocol"] {
  if (typeof p === "string" && (FRONTEND_PROTOCOLS as string[]).includes(p)) {
    return p as TeamTemplate["protocol"];
  }
  return "sequential";
}

function normalizeTemplate(raw: unknown): TeamTemplate | null {
  if (!raw || typeof raw !== "object") return null;
  const r = raw as Record<string, unknown>;
  const id = String(r.id ?? "");
  const name = String(r.name ?? "");
  const description = String(r.description ?? "");

  const nested = r.template;
  if (nested && typeof nested === "object") {
    const t = nested as Record<string, unknown>;
    const membersRaw = t.members;
    const members = Array.isArray(membersRaw) ? membersRaw.map(normalizeMember) : [];
    const protocol = coordinationToProtocol(t.coordination);
    const budget = t.budget as Record<string, unknown> | undefined;
    const maxCost =
      typeof budget?.max_total_cost_usd === "number" ? budget.max_total_cost_usd : 1;
    const estimatedCostPerRun = Math.round(Math.min(Number(maxCost) * 0.1, 5) * 100) / 100 || 0.5;
    const out: TeamTemplate = {
      id: id || String(t.id ?? ""),
      name: name || String(t.name ?? ""),
      description: description || String(t.description ?? ""),
      category: typeof r.category === "string" ? r.category : "Engineering",
      protocol,
      members,
      estimatedCostPerRun,
      tags: Array.isArray(r.tags) ? (r.tags as string[]) : [],
    };
    return out.id ? out : null;
  }

  const membersRaw = r.members;
  const members = Array.isArray(membersRaw) ? membersRaw.map(normalizeMember) : [];
  const out: TeamTemplate = {
    id,
    name,
    description,
    category: typeof r.category === "string" ? r.category : "Engineering",
    protocol: coerceProtocol(r.protocol),
    members,
    estimatedCostPerRun:
      typeof r.estimatedCostPerRun === "number" && !Number.isNaN(r.estimatedCostPerRun)
        ? r.estimatedCostPerRun
        : 0.5,
    tags: Array.isArray(r.tags) ? (r.tags as string[]) : [],
  };
  return out.id ? out : null;
}

function normalizeTemplatesPayload(data: unknown): TeamTemplate[] {
  if (!Array.isArray(data)) return [];
  return data.map(normalizeTemplate).filter((t): t is TeamTemplate => t !== null);
}

function configToTeamDefinition(config: TeamConfig): unknown {
  const id = `team_${Date.now()}`;
  const memberIds = config.members.map((_, i) => `member_${i}`);
  const leadId = memberIds[0] ?? "member_0";

  let coordination: unknown;
  switch (config.protocol) {
    case "hierarchical":
      coordination = { protocol: "hierarchical", lead_id: leadId };
      break;
    case "parallel":
      coordination = { protocol: "parallel", coordinator_id: leadId };
      break;
    case "sequential":
      coordination = { protocol: "sequential", order: memberIds };
      break;
    case "debate":
    case "consensus":
      coordination = { protocol: "consensus", quorum: 0.6, max_rounds: 3 };
      break;
    case "swarm":
      coordination = { protocol: "swarm", max_concurrent: config.members.length };
      break;
    default:
      coordination = { protocol: "sequential", order: memberIds };
  }

  const members = config.members.map((m, i) => ({
    id: memberIds[i],
    role: m.role === "Lead" ? "Lead"
      : m.role === "Reviewer" ? "Reviewer"
      : m.role === "Coordinator" ? "Coordinator"
      : m.role === "Observer" ? "Observer"
      : { Custom: { name: m.role, description: m.role } },
    agent: {
      id: memberIds[i],
      name: m.name,
      system_prompt: m.systemPrompt || `You are ${m.name}, a ${m.role}.`,
      skills: [],
      tools: m.tools,
      model_preference: { model: m.model, provider: m.model.startsWith("claude") ? "anthropic" : "openai" },
      execution_mode: { mode: "interactive" },
      max_iterations: 20,
      timeout_secs: config.maxDuration ?? 600,
    },
    responsibilities: [`Perform ${m.role} tasks`],
    can_delegate_to: [],
    can_veto: m.role === "Lead",
  }));

  return {
    team: {
      id,
      name: config.name,
      description: config.description ?? "",
      members,
      coordination,
      budget: {
        max_total_cost_usd: config.budget,
        max_cost_per_member_usd: config.budget / Math.max(config.members.length, 1),
        max_total_tokens: 500000,
        max_duration_secs: config.maxDuration ?? 600,
        max_iterations_per_member: 20,
      },
      hitl: { enabled: false, checkpoints: [{ checkpoint: "never" }] },
      completion: {
        all_tasks_done: true,
        lead_approval_required: false,
        all_reviews_passed: false,
        completion_phrase: null,
      },
    },
  };
}

function responseToTeam(raw: unknown): Team {
  const r = raw as Record<string, unknown>;
  const t = (r.team ?? r) as Record<string, unknown>;
  const budget = t.budget as Record<string, unknown> | undefined;
  const coord = t.coordination as Record<string, unknown> | undefined;
  const members = Array.isArray(t.members) ? t.members.map(normalizeMember) : [];
  return {
    id: String(t.id ?? ""),
    name: String(t.name ?? ""),
    description: typeof t.description === "string" ? t.description : undefined,
    protocol: coordinationToProtocol(coord),
    members,
    budget: typeof budget?.max_total_cost_usd === "number" ? budget.max_total_cost_usd : 1,
    createdAt: typeof t.created_at === "string" ? t.created_at : new Date().toISOString(),
  };
}

export const teamApi = {
  // Templates
  listTemplates: async (): Promise<TeamTemplate[]> => {
    const raw = await get<unknown>("/teams/templates");
    return normalizeTemplatesPayload(raw);
  },

  // Teams
  listTeams: async (): Promise<Team[]> => {
    const raw = await get<unknown[]>("/teams");
    return Array.isArray(raw) ? raw.map(responseToTeam) : [];
  },
  createTeam: async (config: TeamConfig): Promise<Team> => {
    const raw = await post<unknown>("/teams", configToTeamDefinition(config));
    return responseToTeam(raw);
  },
  getTeam: async (teamId: string): Promise<Team> => {
    const raw = await get<unknown>(`/teams/${teamId}`);
    return responseToTeam(raw);
  },
  deleteTeam: (teamId: string) => del(`/teams/${teamId}`),

  // Runs
  startRun: (teamId: string, task: string) =>
    post<TeamRun>(`/teams/${teamId}/runs`, { task }),
  listRuns: (teamId: string) => get<TeamRun[]>(`/teams/${teamId}/runs`),
  getRun: (runId: string) => get<TeamRun>(`/teams/runs/${runId}`),
  getRunResult: (runId: string) => get<TeamRunResult>(`/teams/runs/${runId}/result`),

  // Marketplace — align with the real backend routes under
  // /marketplace/community/* (the legacy /marketplace/contributions/* paths
  // returned 404). Search uses a query param, not a POST body.
  listMarketplace: () =>
    get<MarketplaceContribution[]>("/marketplace/community"),
  searchMarketplace: (query: string) =>
    get<MarketplaceContribution[]>(
      `/marketplace/community/search?q=${encodeURIComponent(query)}`,
    ),
  installContribution: (id: string) =>
    post<{ status: string }>(`/marketplace/community/contributions/${id}/install`),
};
