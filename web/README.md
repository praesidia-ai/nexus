# Nexus Web

Standalone Next.js 15 frontend for the Nexus AI business OS.
Connects to the `nexus-http` Axum API server.

## Quick start

```bash
# 1. Start the API server (from nexus-rust/)
OPENAI_API_KEY=sk-... cargo run -p nexus-http

# 2. Install and run the web app
cd nexus-rust/web
npm install
cp .env.local.example .env.local
npm run dev          # http://localhost:8080
```

## Environment

| Variable | Default | Description |
|----------|---------|-------------|
| `NEXUS_API_URL` | `http://localhost:8020` | nexus-http API URL (server-side) |
| `NEXT_PUBLIC_USER_NAME` | — | Display name in sidebar |

## Pages

| Route | Description |
|-------|-------------|
| `/` | Home: "What are you working on?" |
| `/[projectId]/chat` | SSE streaming chat with inline action cards |
| `/[projectId]/dashboard` | KPIs, activity chart, agent pipeline |
| `/[projectId]/knowledge` | Knowledge base items |
| `/[projectId]/databases` | Database tables |
| `/[projectId]/databases/[tableId]` | Table records (spreadsheet view) |
| `/[projectId]/agents` | Agent definitions (run/stop/delete) |
| `/[projectId]/vault` | Secret key/value store |
| `/portal/[slug]` | Published client portals (served by nexus-http) |

## Tech stack

- **Next.js 15** App Router + React 19
- **Tailwind CSS 3** for styling
- **Lucide React** for icons
- **Server-sent events** (fetch + ReadableStream) for streaming chat
