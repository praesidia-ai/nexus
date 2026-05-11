// Nexus Forge reputation client.
//
// Wraps the public, auth-free gallery surfaces exposed by
// `handlers/forge.rs`:
//
//   GET /forge/trending   — installs_7d × avg_rating
//   GET /forge/top-rated  — Bayesian prior of 5 neutral reviews
//   GET /forge/newest     — most recently created listings
//
// Install and review endpoints are project-scoped and auth-gated,
// so they live on `api.ts` (alongside the rest of the authenticated
// surface). This file is read-only.

import { BASE } from "./api";

export type ForgeSurface = "trending" | "top_rated" | "newest";

export interface ForgeListing {
  id: string;
  item_type: "agent" | "plugin" | "workflow" | "tool" | string;
  name: string;
  description: string;
  author: string;
  version: string;
  icon?: string | null;
  tags?: string | null;
  is_official: boolean;
  install_count: number;
  install_count_7d: number;
  review_count: number;
  avg_rating: number;
  /** Wire score — numeric for trending/top_rated, ISO-8601 string for newest. */
  score: number | string;
}

interface GalleryResponse {
  surface: ForgeSurface;
  items: ForgeListing[];
}

export interface GalleryOptions {
  limit?: number;
  itemType?: string;
}

async function fetchGallery(
  surface: ForgeSurface,
  opts: GalleryOptions = {},
): Promise<ForgeListing[]> {
  const path = surface === "top_rated" ? "top-rated" : surface;
  const qs = new URLSearchParams();
  if (opts.limit !== undefined) qs.set("limit", String(opts.limit));
  if (opts.itemType) qs.set("item_type", opts.itemType);
  const url = `${BASE}/forge/${path}${qs.toString() ? `?${qs.toString()}` : ""}`;
  const res = await fetch(url, { headers: { accept: "application/json" } });
  if (!res.ok) {
    throw new Error(`forge ${surface} — HTTP ${res.status}`);
  }
  const data = (await res.json()) as GalleryResponse;
  return Array.isArray(data.items) ? data.items : [];
}

export const forgeApi = {
  trending: (opts?: GalleryOptions) => fetchGallery("trending", opts),
  topRated: (opts?: GalleryOptions) => fetchGallery("top_rated", opts),
  newest: (opts?: GalleryOptions) => fetchGallery("newest", opts),
};
