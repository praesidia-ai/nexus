# nexus-sdk

Python SDK for the Nexus AI agent platform.

## Installation

```bash
pip install nexus-sdk
```

## Quick Start

```python
from nexus_sdk import NexusClient

client = NexusClient("http://localhost:8020", "your-api-key")

# Generate a full application
result = client.generate("Build a todo app with dark mode")
print(f"Project: {result.project_id}, Taste: {result.taste_score}")

# Run a single agent
agent = client.run_agent("coder", "Add user authentication")
print(f"Modified {len(agent.files_modified)} files")

# Run a multi-agent team
team = client.run_team("full-stack", "Build an e-commerce checkout")
print(f"Cost: ${team.cost_usd}, Messages: {team.messages_count}")

# Score quality
quality = client.score_quality(result.project_id)
print(f"Overall: {quality.overall}")

# Analyze intent
intent = client.analyze_intent("A fitness tracking dashboard")
print(f"Type: {intent.app_type}, Complexity: {intent.complexity}")

# Memory operations
client.remember("User prefers React + Tailwind", tags=["preferences", "stack"])
memories = client.recall("preferred stack", limit=5)

# Health check
health = client.health()
print(health)
```

## API Reference

### `NexusClient(base_url, api_key)`

| Method | Description |
|--------|-------------|
| `generate(description)` | Generate a full app from a description |
| `run_agent(role, task)` | Run a single coding agent |
| `run_team(template, task)` | Run a multi-agent team |
| `score_quality(project_id)` | Score project quality via taste engine |
| `analyze_intent(description)` | Analyze intent from description |
| `remember(content, tags)` | Store content in memory |
| `recall(query, limit)` | Recall memories by semantic query |
| `health()` | Check server health |
