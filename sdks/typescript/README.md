# @nexus-ai/sdk

TypeScript SDK for the Nexus AI agent platform.

## Installation

```bash
npm install @nexus-ai/sdk
```

## Quick Start

```typescript
import { NexusClient } from '@nexus-ai/sdk';

const client = new NexusClient('http://localhost:8020', 'your-api-key');

// Generate a full application
const result = await client.generate('Build a todo app with dark mode');
console.log(`Project: ${result.project_id}, Taste: ${result.taste_score}`);

// Run a single agent
const agent = await client.runAgent('coder', 'Add user authentication');
console.log(`Modified ${agent.files_modified.length} files`);

// Run a multi-agent team
const team = await client.runTeam('full-stack', 'Build an e-commerce checkout');
console.log(`Cost: $${team.cost_usd}, Messages: ${team.messages_count}`);

// Score quality
const quality = await client.scoreQuality(result.project_id);
console.log(`Overall: ${quality.overall}, Layout: ${quality.axes.layout}`);

// Analyze intent
const intent = await client.analyzeIntent('A fitness tracking dashboard');
console.log(`Type: ${intent.app_type}, Complexity: ${intent.complexity}`);

// Memory operations
await client.remember('User prefers React + Tailwind', ['preferences', 'stack']);
const memories = await client.recall('preferred stack', 5);

// Health check
const health = await client.health();
console.log(health);
```

## API Reference

### `NexusClient`

| Method | Description |
|--------|-------------|
| `generate(description)` | Generate a full app from a description |
| `runAgent(role, task)` | Run a single coding agent |
| `runTeam(template, task)` | Run a multi-agent team |
| `scoreQuality(projectId)` | Score project quality via taste engine |
| `analyzeIntent(description)` | Analyze intent from description |
| `remember(content, tags)` | Store content in memory |
| `recall(query, limit)` | Recall memories by semantic query |
| `health()` | Check server health |
