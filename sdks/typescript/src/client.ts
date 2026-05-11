import type {
  GenerationResult,
  AgentResult,
  TeamResult,
  QualityScore,
  IntentResult,
  MemoryEntry,
} from './types';

/**
 * TypeScript client for the Nexus AI platform REST API.
 *
 * Uses native `fetch()` with no external dependencies.
 *
 * @example
 * ```ts
 * const client = new NexusClient('http://localhost:8020', 'your-api-key');
 * const result = await client.generate('Build a todo app');
 * console.log(result.project_id, result.taste_score);
 * ```
 */
export class NexusClient {
  private readonly baseUrl: string;
  private readonly apiKey: string;

  /**
   * Create a new NexusClient.
   * @param baseUrl - Root URL of the Nexus HTTP server (e.g. "http://localhost:8020").
   * @param apiKey - API key from Nexus security settings.
   */
  constructor(baseUrl: string, apiKey: string) {
    this.baseUrl = baseUrl.replace(/\/+$/, '');
    this.apiKey = apiKey;
  }

  // ── Generation ──────────────────────────────────────────────────────────

  /** Generate a full application from a natural-language description. */
  async generate(description: string): Promise<GenerationResult> {
    return this.post<GenerationResult>('/oneshot/sync', { description });
  }

  // ── Agents ──────────────────────────────────────────────────────────────

  /**
   * Run a single coding agent on a task within a project.
   *
   * **Note:** This endpoint returns an SSE stream. This client awaits the
   * final JSON summary. For real-time streaming, use an SSE-capable client.
   *
   * @param projectId - The project to run the agent against.
   * @param role - Agent role (e.g. "architect", "coder", "reviewer").
   * @param task - Natural-language task description.
   */
  async runAgent(projectId: string, role: string, task: string): Promise<AgentResult> {
    return this.post<AgentResult>(`/projects/${projectId}/coding-agent/run`, { role, task });
  }

  // ── Teams ───────────────────────────────────────────────────────────────

  /**
   * Run a multi-agent team on a task.
   *
   * **Note:** This endpoint returns an SSE stream. This client awaits the
   * final JSON summary. For real-time streaming, use an SSE-capable client.
   *
   * @param teamId - The team ID (create one first via the teams API).
   * @param task - Natural-language task description.
   */
  async runTeam(teamId: string, task: string): Promise<TeamResult> {
    return this.post<TeamResult>(`/teams/${teamId}/run`, { task });
  }

  // ── Quality ─────────────────────────────────────────────────────────────

  /** Score the quality of a generated project using the taste engine. */
  async scoreQuality(projectId: string): Promise<QualityScore> {
    return this.get<QualityScore>(`/projects/${projectId}/taste`);
  }

  // ── Intent ──────────────────────────────────────────────────────────────

  /** Analyze a natural-language description to extract intent metadata. */
  async analyzeIntent(description: string): Promise<IntentResult> {
    return this.post<IntentResult>('/intent/analyze', { description });
  }

  // ── Memory ──────────────────────────────────────────────────────────────

  /** Store content in the persistent memory system. */
  async remember(content: string, tags: string[]): Promise<void> {
    await this.post('/memory', { content, tags });
  }

  /** Recall memories matching a semantic query. */
  async recall(query: string, limit: number): Promise<MemoryEntry[]> {
    const resp = await this.post<MemoryEntry[] | { results: MemoryEntry[] }>(
      '/memory/knowledge/query',
      { query, limit },
    );
    if (Array.isArray(resp)) return resp;
    if ('results' in resp) return resp.results;
    return [];
  }

  // ── Health ──────────────────────────────────────────────────────────────

  /** Check the health of the Nexus server. */
  async health(): Promise<Record<string, unknown>> {
    return this.get<Record<string, unknown>>('/health');
  }

  // ── A2A Protocol ────────────────────────────────────────────────────────

  /** Retrieve the local agent's A2A Agent Card. */
  async agentCard(): Promise<Record<string, unknown>> {
    return this.get('/.well-known/agent.json');
  }

  /** Dispatch a task to a remote A2A agent. */
  async a2aDispatch(message: string): Promise<Record<string, unknown>> {
    return this.post('/a2a', {
      jsonrpc: '2.0',
      id: crypto.randomUUID(),
      method: 'tasks/send',
      params: {
        message: { role: 'user', parts: [{ kind: 'text', text: message }] },
      },
    });
  }

  /** List known A2A peer agents. */
  async listA2aPeers(): Promise<unknown[]> {
    return this.get('/a2a/peers');
  }

  /** Register a new A2A peer by URL. */
  async discoverA2aPeer(url: string): Promise<Record<string, unknown>> {
    return this.post('/a2a/peers/discover', { url });
  }

  // ── Marketplace ─────────────────────────────────────────────────────────

  /** Search the community agent/plugin marketplace. */
  async searchMarketplace(params?: {
    q?: string;
    kind?: 'agent' | 'plugin' | 'workflow' | 'tool';
    sort?: 'downloads' | 'rating' | 'updated' | 'name';
    page?: number;
    limit?: number;
  }): Promise<Record<string, unknown>> {
    const qs = new URLSearchParams();
    if (params?.q) qs.set('q', params.q);
    if (params?.kind) qs.set('kind', params.kind);
    if (params?.sort) qs.set('sort', params.sort);
    if (params?.page) qs.set('page', String(params.page));
    if (params?.limit) qs.set('limit', String(params.limit));
    const suffix = qs.toString() ? `?${qs}` : '';
    return this.get(`/marketplace/search${suffix}`);
  }

  /** Install a package from the marketplace. */
  async installPackage(name: string, version?: string): Promise<Record<string, unknown>> {
    return this.post('/marketplace/install', { name, version });
  }

  /** Uninstall a marketplace package. */
  async uninstallPackage(name: string): Promise<Record<string, unknown>> {
    return this.post('/marketplace/uninstall', { name });
  }

  /** List locally installed marketplace packages. */
  async listInstalledPackages(): Promise<unknown[]> {
    return this.get('/marketplace/installed');
  }

  // ── Workflows ───────────────────────────────────────────────────────────

  /** List workflow runs for a project. */
  async listWorkflows(projectId: string): Promise<unknown[]> {
    return this.get(`/projects/${projectId}/workflows`);
  }

  /** Trigger a named workflow. */
  async runWorkflow(
    projectId: string,
    name: string,
    payload?: Record<string, unknown>,
  ): Promise<Record<string, unknown>> {
    return this.post(`/projects/${projectId}/workflows/run`, { name, payload });
  }

  // ── Governance ──────────────────────────────────────────────────────────

  /** List all governance policies. */
  async listPolicies(): Promise<unknown[]> {
    return this.get('/governance/policies');
  }

  /** Evaluate an agent action against governance policies. */
  async evaluateAction(
    action: string,
    agentId?: string,
    context?: Record<string, unknown>,
  ): Promise<Record<string, unknown>> {
    return this.post('/governance/evaluate', { action, agent_id: agentId, context });
  }

  /** Redact PII from a block of text. */
  async redactPii(text: string): Promise<{ redacted: string; count: number }> {
    return this.post('/governance/pii/redact', { text });
  }

  // ── Audit Trail ─────────────────────────────────────────────────────────

  /** List cryptographic audit log entries. */
  async listAuditEntries(): Promise<unknown[]> {
    return this.get('/audit/chain');
  }

  /** Verify the integrity of the audit chain. */
  async verifyAuditChain(): Promise<{ valid: boolean; length: number }> {
    return this.get('/audit/chain/verify');
  }

  // ── Internal helpers ──────────────────────────────────────────────────

  private async get<T>(path: string): Promise<T> {
    const url = `${this.baseUrl}${path}`;
    const resp = await fetch(url, {
      method: 'GET',
      headers: {
        Authorization: `Bearer ${this.apiKey}`,
        'Content-Type': 'application/json',
      },
    });
    return this.handleResponse<T>(resp);
  }

  private async post<T>(path: string, body: unknown): Promise<T> {
    const url = `${this.baseUrl}${path}`;
    const resp = await fetch(url, {
      method: 'POST',
      headers: {
        Authorization: `Bearer ${this.apiKey}`,
        'Content-Type': 'application/json',
      },
      body: JSON.stringify(body),
    });
    return this.handleResponse<T>(resp);
  }

  private async handleResponse<T>(resp: Response): Promise<T> {
    if (resp.status === 401 || resp.status === 403) {
      const text = await resp.text();
      throw new Error(`Authentication failed: ${text}`);
    }
    if (!resp.ok) {
      const text = await resp.text();
      throw new Error(`API error (status ${resp.status}): ${text}`);
    }
    const text = await resp.text();
    if (!text) return null as unknown as T;
    return JSON.parse(text) as T;
  }
}
