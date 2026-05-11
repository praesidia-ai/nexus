Build or modify a UI feature in the nexus-rust web/ frontend.

Read `.claude/skills/frontend/SKILL.md` first, then follow the appropriate path:

**Adding a new page**:
1. Create `web/src/app/[projectId]/<feature>/page.tsx` with `"use client"` directive
2. Add `loading.tsx` and `error.tsx` siblings (copy from an existing route)
3. Add the route to the project tab bar in `app/[projectId]/page.tsx` if it's a project-scoped view

**Adding a new API hook**:
1. Create `web/src/hooks/api/use-<feature>.ts` following the Tanstack Query pattern
2. Add the TypeScript response interface to `web/src/lib/api.ts`
3. Add the fetch method to the `api` object in `lib/api.ts`

**Adding SSE streaming UI**:
1. Use the `useSSE` hook from `@/hooks/useSSE`
2. Pass `url: null` when not active, the endpoint when active
3. Close the stream (`close()`) when receiving a `complete` or `error` event type

**Rules**:
- No `any` — define proper TypeScript interfaces for all API responses
- All API calls go through `lib/api.ts` — never fetch directly in components
- Use `/api/...` paths — the Next.js proxy rewrites to `:8080` automatically
- Use shadcn/ui components from `@/components/ui/` — don't add new UI libraries
- Run `npm run build` in `web/` to catch TypeScript errors before finishing

**Dev**: `cd web && npm run dev` (requires backend running on :8080)
