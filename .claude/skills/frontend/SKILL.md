---
name: frontend
description: Build and modify UI features in the nexus-rust web/ Next.js frontend. Use when adding pages, components, hooks, or wiring new backend endpoints to the UI.
---

# Frontend — nexus-rust web/

## Stack

- **Framework**: Next.js 14+ App Router (`web/src/app/`)
- **Language**: TypeScript strict mode — no `any`, no `@ts-ignore`
- **UI**: Tailwind CSS + shadcn/ui components (`components/ui/`)
- **Data fetching**: Tanstack Query (`@tanstack/react-query`) via hooks in `hooks/api/`
- **SSE streaming**: custom `useSSE` hook (`hooks/useSSE.ts`)
- **API client**: `lib/api.ts` — all backend calls go through the `api` object
- **State**: React hooks + Zustand stores (`stores/`)
- **Icons**: `lucide-react`

## Proxy setup

All `/api/*` requests are proxied to `http://localhost:8080` via `next.config.ts`. In components, use `/api/...` paths — never hardcode the backend port.

```ts
// CORRECT
fetch("/api/projects")

// WRONG — breaks in production
fetch("http://localhost:8080/projects")
```

In server components / Node.js context, `BASE` falls back to `NEXUS_API_URL` env var.

---

## Adding a new page

Pages live under `web/src/app/`. Follow the existing structure:

```
web/src/app/
  [projectId]/
    my-feature/
      page.tsx       ← "use client" if it needs hooks/state
      loading.tsx    ← Suspense fallback (copy from another loading.tsx)
      error.tsx      ← Error boundary (copy from another error.tsx)
```

Page skeleton:

```tsx
"use client";

import { useParams } from "next/navigation";
import { useMyFeature } from "@/hooks/api/use-my-feature";

export default function MyFeaturePage() {
  const { projectId } = useParams<{ projectId: string }>();
  const { data, isLoading, error } = useMyFeature(projectId);

  if (isLoading) return <div className="p-6 text-muted-foreground">Loading...</div>;
  if (error) return <div className="p-6 text-destructive">Failed to load.</div>;

  return (
    <div className="p-6">
      {/* content */}
    </div>
  );
}
```

---

## Adding a new API hook

Hooks in `hooks/api/` follow the Tanstack Query pattern:

```ts
// web/src/hooks/api/use-my-feature.ts
import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { api } from "@/lib/api";

// Query — read data
export function useMyFeature(projectId: string) {
  return useQuery({
    queryKey: ["my-feature", projectId],
    queryFn: () => api.getMyFeature(projectId),
    enabled: !!projectId,
  });
}

// Mutation — write data
export function useCreateMyItem(projectId: string) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (data: CreateMyItemDto) => api.createMyItem(projectId, data),
    onSuccess: () => {
      // Invalidate so the list refetches
      queryClient.invalidateQueries({ queryKey: ["my-feature", projectId] });
    },
  });
}
```

---

## Adding a new API method to lib/api.ts

`lib/api.ts` exports an `api` object with all backend calls. Add new methods following the existing patterns:

### 1. Add the TypeScript interface for the response type

```ts
// Near the top with other interfaces
export interface MyFeatureItem {
  id: string;
  project_id: string;
  name: string;
  created_at: string;
}
```

### 2. Add the method to the `api` object

```ts
// In the api object at the bottom of lib/api.ts
async getMyFeature(projectId: string): Promise<MyFeatureItem[]> {
  const res = await fetch(`${BASE}/projects/${projectId}/my-feature`);
  if (!res.ok) throw new Error(`Failed to fetch my-feature: ${res.status}`);
  return res.json();
},

async createMyItem(projectId: string, data: { name: string }): Promise<MyFeatureItem> {
  const res = await fetch(`${BASE}/projects/${projectId}/my-feature`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(data),
  });
  if (!res.ok) throw new Error(`Failed to create: ${res.status}`);
  return res.json();
},
```

---

## Consuming SSE streams

Use the `useSSE` hook (`hooks/useSSE.ts`) for real-time backend streams:

```tsx
"use client";

import { useState } from "react";
import { useSSE } from "@/hooks/useSSE";

// Define the event shape (must match the Rust SSE event enum)
interface MyStreamEvent {
  type: "phase" | "progress" | "complete" | "error";
  message?: string;
  percent?: number;
  result?: string;
}

export function MyStreamingComponent({ projectId }: { projectId: string }) {
  const [events, setEvents] = useState<MyStreamEvent[]>([]);
  const [isRunning, setIsRunning] = useState(false);

  // useSSE auto-reconnects with exponential backoff (1s→2s→4s, max 10 retries)
  const { status, close } = useSSE<MyStreamEvent>(
    isRunning ? `/api/projects/${projectId}/my-stream` : null,
    (event) => {
      setEvents((prev) => [...prev, event]);

      // Close the stream on terminal events
      if (event.type === "complete" || event.type === "error") {
        setIsRunning(false);
        close();
      }
    },
    { reconnect: true, maxReconnects: 10 }
  );

  return (
    <div>
      <button onClick={() => setIsRunning(true)}>Start</button>
      <div>Status: {status}</div>
      {events.map((e, i) => (
        <div key={i}>{e.message}</div>
      ))}
    </div>
  );
}
```

For POST-triggered SSE streams (like oneshot), use `streamOneshot` in `lib/api.ts` as the pattern — it uses `fetch` with `ReadableStream` to handle POST+SSE.

---

## Using shadcn/ui components

All UI primitives are in `components/ui/`. Use them directly:

```tsx
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { Input } from "@/components/ui/input";
import { Dialog, DialogContent, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { Tabs, TabsList, TabsTrigger, TabsContent } from "@/components/ui/tabs";

// Tailwind utilities
import { cn } from "@/lib/utils";  // classnames merge helper
```

Do not install new UI component libraries without good reason — use what's already there.

---

## Routing and navigation

- Use `useParams()` for route params — never `window.location`
- Use `useRouter()` + `router.push()` for navigation
- Project routes: `/[projectId]/...`
- Add new project sub-routes to the tab bar in `app/[projectId]/page.tsx` or as new routes

---

## TypeScript rules

- No `any` — use `unknown` + type narrowing or define proper interfaces
- No `@ts-ignore` — fix the type error
- Prefer `interface` for object shapes, `type` for unions/aliases
- Use `Record<string, unknown>` for dynamic JSON blobs
- All API response types must be defined in `lib/api.ts` as exported interfaces

---

## Dev workflow

```bash
cd nexus-rust/web
npm run dev      # frontend dev server (port 3000, proxies to :8080)
npm run build    # production build — catches TS errors
npm run lint     # ESLint check
```

The backend must be running on `:8080` for API calls to work in dev.
