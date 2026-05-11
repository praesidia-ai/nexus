"""Response types returned by the Nexus API."""

from dataclasses import dataclass, field
from typing import Dict, List, Optional


@dataclass
class GenerationResult:
    """Result of a full app generation via /oneshot."""

    project_id: str
    """Unique identifier for the generated project."""
    project_name: str
    """Human-readable project name."""
    taste_score: float
    """Taste quality score (0.0 to 1.0)."""
    files_count: int
    """Number of files generated."""
    duration_ms: int
    """Total generation time in milliseconds."""
    app_url: Optional[str] = None
    """URL where the running app can be accessed, if available."""


@dataclass
class AgentResult:
    """Result of running a single agent on a task."""

    output: str
    """The agent's textual output."""
    files_modified: List[str] = field(default_factory=list)
    """Files that were created or modified by the agent."""
    iterations_used: int = 0
    """Number of LLM iterations the agent used."""
    duration_ms: int = 0
    """Total execution time in milliseconds."""


@dataclass
class TeamResult:
    """Result of running a multi-agent team on a task."""

    artifacts: List[str] = field(default_factory=list)
    """Artifacts produced by the team."""
    messages_count: int = 0
    """Total number of inter-agent messages exchanged."""
    cost_usd: float = 0.0
    """Estimated USD cost of all LLM calls."""
    duration_ms: int = 0
    """Total execution time in milliseconds."""


@dataclass
class QualityScore:
    """Quality assessment of a generated project."""

    overall: float = 0.0
    """Overall quality score (0.0 to 1.0)."""
    axes: Dict[str, float] = field(default_factory=dict)
    """Per-axis scores (e.g. 'layout', 'typography', 'color')."""


@dataclass
class IntentResult:
    """Result of intent analysis on a natural-language description."""

    app_type: str = ""
    """Detected application type."""
    complexity: str = ""
    """Estimated complexity."""
    domain: str = ""
    """Application domain."""
    confidence: float = 0.0
    """Confidence of the intent analysis (0.0 to 1.0)."""


@dataclass
class MemoryEntry:
    """A single entry from the persistent memory system."""

    id: str = ""
    """Unique identifier for this memory entry."""
    content: str = ""
    """The stored content."""
    category: str = ""
    """Category or classification of the memory."""
    confidence: float = 0.0
    """Confidence or relevance score (0.0 to 1.0)."""
