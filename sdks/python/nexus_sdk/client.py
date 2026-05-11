"""HTTP client for the Nexus AI platform REST API.

Uses only the standard library (urllib) so there are zero required
dependencies. Install `aiohttp` for async support in the future.

Example::

    from nexus_sdk import NexusClient

    client = NexusClient("http://localhost:8020", "your-api-key")
    result = client.generate("Build a todo app")
    print(result.project_id, result.taste_score)
"""

import json
import urllib.parse
import urllib.request
import urllib.error
from typing import Any, Dict, List, Optional

from nexus_sdk.types import (
    AgentResult,
    GenerationResult,
    IntentResult,
    MemoryEntry,
    QualityScore,
    TeamResult,
)


class NexusClient:
    """Typed HTTP client for the Nexus REST API."""

    def __init__(self, base_url: str, api_key: str) -> None:
        """Create a new client.

        Args:
            base_url: Root URL of the Nexus server (e.g. "http://localhost:8020").
            api_key: API key from Nexus security settings.
        """
        self.base_url = base_url.rstrip("/")
        self.api_key = api_key

    # ── Generation ──────────────────────────────────────────────────────────

    def generate(self, description: str) -> GenerationResult:
        """Generate a full application from a natural-language description."""
        data = self._post("/oneshot/sync", {"description": description})
        return GenerationResult(
            project_id=data.get("project_id", ""),
            project_name=data.get("project_name", ""),
            taste_score=data.get("taste_score", 0.0),
            files_count=data.get("files_count", 0),
            duration_ms=data.get("duration_ms", 0),
            app_url=data.get("app_url"),
        )

    # ── Agents ──────────────────────────────────────────────────────────────

    def run_agent(self, project_id: str, role: str, task: str) -> AgentResult:
        """Run a single coding agent on a task within a project.

        Note: This endpoint returns an SSE stream. This client awaits the
        final JSON summary. For real-time streaming, use an SSE-capable
        HTTP library instead.

        Args:
            project_id: The project to run the agent against.
            role: Agent role (e.g. "architect", "coder", "reviewer").
            task: Natural-language task description.
        """
        data = self._post(
            f"/projects/{project_id}/coding-agent/run",
            {"role": role, "task": task},
        )
        return AgentResult(
            output=data.get("output", ""),
            files_modified=data.get("files_modified", []),
            iterations_used=data.get("iterations_used", 0),
            duration_ms=data.get("duration_ms", 0),
        )

    # ── Teams ───────────────────────────────────────────────────────────────

    def run_team(self, team_id: str, task: str) -> TeamResult:
        """Run a multi-agent team on a task.

        Note: This endpoint returns an SSE stream. This client awaits the
        final JSON summary. For real-time streaming, use an SSE-capable
        HTTP library instead.

        Args:
            team_id: The team ID (create one first via the teams API).
            task: Natural-language task description.
        """
        data = self._post(f"/teams/{team_id}/run", {"task": task})
        return TeamResult(
            artifacts=data.get("artifacts", []),
            messages_count=data.get("messages_count", 0),
            cost_usd=data.get("cost_usd", 0.0),
            duration_ms=data.get("duration_ms", 0),
        )

    # ── Quality ─────────────────────────────────────────────────────────────

    def score_quality(self, project_id: str) -> QualityScore:
        """Score the quality of a generated project."""
        data = self._get(f"/projects/{project_id}/taste")
        return QualityScore(
            overall=data.get("overall", 0.0),
            axes=data.get("axes", {}),
        )

    # ── Intent ──────────────────────────────────────────────────────────────

    def analyze_intent(self, description: str) -> IntentResult:
        """Analyze a natural-language description to extract intent metadata."""
        data = self._post("/intent/analyze", {"description": description})
        return IntentResult(
            app_type=data.get("app_type", ""),
            complexity=data.get("complexity", ""),
            domain=data.get("domain", ""),
            confidence=data.get("confidence", 0.0),
        )

    # ── Memory ──────────────────────────────────────────────────────────────

    def remember(self, content: str, tags: Optional[List[str]] = None) -> None:
        """Store content in the persistent memory system."""
        self._post("/memory", {"content": content, "tags": tags or []})

    def recall(self, query: str, limit: int = 10) -> List[MemoryEntry]:
        """Recall memories matching a semantic query."""
        data = self._post("/memory/knowledge/query", {"query": query, "limit": limit})
        items = data if isinstance(data, list) else data.get("results", [])
        return [
            MemoryEntry(
                id=item.get("id", ""),
                content=item.get("content", ""),
                category=item.get("category", ""),
                confidence=item.get("confidence", 0.0),
            )
            for item in items
        ]

    # ── Health ──────────────────────────────────────────────────────────────

    def health(self) -> Dict[str, Any]:
        """Check the health of the Nexus server."""
        return self._get("/health")

    # ── A2A Protocol ────────────────────────────────────────────────────────

    def agent_card(self) -> Dict[str, Any]:
        """Retrieve the local agent's A2A Agent Card."""
        return self._get("/.well-known/agent.json")

    def a2a_dispatch(self, message: str) -> Dict[str, Any]:
        """Dispatch a task to the local A2A agent endpoint."""
        import uuid
        return self._post("/a2a", {
            "jsonrpc": "2.0",
            "id": str(uuid.uuid4()),
            "method": "tasks/send",
            "params": {
                "message": {"role": "user", "parts": [{"kind": "text", "text": message}]},
            },
        })

    def list_a2a_peers(self) -> List[Dict[str, Any]]:
        """List registered A2A peer agents."""
        data = self._get("/a2a/peers")
        return data if isinstance(data, list) else []

    def discover_a2a_peer(self, url: str) -> Dict[str, Any]:
        """Discover and register a new A2A peer by URL."""
        return self._post("/a2a/peers/discover", {"url": url})

    # ── Marketplace ─────────────────────────────────────────────────────────

    def search_marketplace(
        self,
        q: Optional[str] = None,
        kind: Optional[str] = None,
        sort: Optional[str] = None,
        page: int = 1,
        limit: int = 24,
    ) -> Dict[str, Any]:
        """Search the community agent/plugin marketplace."""
        params: List[str] = []
        if q:
            params.append(f"q={urllib.parse.quote(q)}")
        if kind:
            params.append(f"kind={kind}")
        if sort:
            params.append(f"sort={sort}")
        params.append(f"page={page}")
        params.append(f"limit={limit}")
        qs = "&".join(params)
        return self._get(f"/marketplace/search?{qs}")

    def install_package(self, name: str, version: Optional[str] = None) -> Dict[str, Any]:
        """Install a package from the marketplace."""
        body: Dict[str, Any] = {"name": name}
        if version:
            body["version"] = version
        return self._post("/marketplace/install", body)

    def uninstall_package(self, name: str) -> Dict[str, Any]:
        """Uninstall a marketplace package."""
        return self._post("/marketplace/uninstall", {"name": name})

    def list_installed_packages(self) -> List[Dict[str, Any]]:
        """List locally installed marketplace packages."""
        data = self._get("/marketplace/installed")
        return data if isinstance(data, list) else []

    # ── Workflows ───────────────────────────────────────────────────────────

    def list_workflows(self, project_id: str) -> List[Dict[str, Any]]:
        """List workflow runs for a project."""
        data = self._get(f"/projects/{project_id}/workflows")
        return data if isinstance(data, list) else []

    def run_workflow(
        self,
        project_id: str,
        name: str,
        payload: Optional[Dict[str, Any]] = None,
    ) -> Dict[str, Any]:
        """Trigger a named workflow in a project."""
        return self._post(f"/projects/{project_id}/workflows/run", {"name": name, "payload": payload})

    # ── Governance ──────────────────────────────────────────────────────────

    def list_policies(self) -> List[Dict[str, Any]]:
        """List all governance policies."""
        data = self._get("/governance/policies")
        return data if isinstance(data, list) else []

    def evaluate_action(
        self,
        action: str,
        agent_id: Optional[str] = None,
        context: Optional[Dict[str, Any]] = None,
    ) -> Dict[str, Any]:
        """Evaluate an agent action against governance policies."""
        return self._post("/governance/evaluate", {
            "action": action,
            "agent_id": agent_id,
            "context": context or {},
        })

    def redact_pii(self, text: str) -> Dict[str, Any]:
        """Redact PII from a block of text."""
        return self._post("/governance/pii/redact", {"text": text})

    # ── Audit Trail ─────────────────────────────────────────────────────────

    def list_audit_entries(self) -> List[Dict[str, Any]]:
        """List cryptographic audit log entries."""
        data = self._get("/audit/chain")
        return data if isinstance(data, list) else []

    def verify_audit_chain(self) -> Dict[str, Any]:
        """Verify the integrity of the audit chain."""
        return self._get("/audit/chain/verify")

    # ── Internal helpers ────────────────────────────────────────────────────

    def _get(self, path: str) -> Any:
        url = f"{self.base_url}{path}"
        req = urllib.request.Request(url, method="GET")
        req.add_header("Authorization", f"Bearer {self.api_key}")
        req.add_header("Content-Type", "application/json")
        return self._do_request(req)

    def _post(self, path: str, body: Any) -> Any:
        url = f"{self.base_url}{path}"
        data = json.dumps(body).encode("utf-8")
        req = urllib.request.Request(url, data=data, method="POST")
        req.add_header("Authorization", f"Bearer {self.api_key}")
        req.add_header("Content-Type", "application/json")
        return self._do_request(req)

    def _do_request(self, req: urllib.request.Request) -> Any:
        try:
            with urllib.request.urlopen(req) as resp:
                body = resp.read().decode("utf-8")
                if not body:
                    return {}
                return json.loads(body)
        except urllib.error.HTTPError as exc:
            body = exc.read().decode("utf-8") if exc.fp else ""
            if exc.code in (401, 403):
                raise PermissionError(f"Authentication failed: {body}") from exc
            raise RuntimeError(f"API error (status {exc.code}): {body}") from exc
