import { BASE } from "./api";

export type PackageKind = "agent" | "plugin" | "workflow" | "tool";
export type PackageRuntime = "wasm" | "a2a" | "native";

export interface MarketplacePackage {
  name: string;
  kind: PackageKind;
  runtime: PackageRuntime;
  latestVersion: string;
  description: string;
  keywords: string[];
  categories: string[];
  downloads: number;
  rating: number | null;
  reviewCount: number;
  author: string;
  createdAt: string;
  updatedAt: string;
  installed: boolean;
  installedVersion?: string;
}

export interface PackageReview {
  id: string;
  author: string;
  rating: number;
  comment: string;
  createdAt: string;
  helpful: number;
}

export interface PackageVersion {
  version: string;
  publishedAt: string;
  yanked: boolean;
  changelog?: string;
}

export interface PackageDetail extends MarketplacePackage {
  readme: string;
  versions: PackageVersion[];
  reviews: PackageReview[];
  capabilities: {
    network: boolean;
    filesystem: boolean;
    exec: boolean;
    mcp: boolean;
    a2a: boolean;
    memory: boolean;
    workflows: boolean;
  };
  resources: {
    maxFuelM?: number;
    maxMemoryMb?: number;
    timeoutSecs?: number;
  };
}

export interface SearchParams {
  q?: string;
  kind?: PackageKind;
  category?: string;
  sort?: "downloads" | "rating" | "updated" | "name";
  page?: number;
  limit?: number;
}

export interface SearchResult {
  packages: MarketplacePackage[];
  total: number;
  page: number;
  totalPages: number;
}

export const marketplaceApi = {
  async search(params: SearchParams = {}): Promise<SearchResult> {
    const query = new URLSearchParams();
    if (params.q) query.set("q", params.q);
    if (params.kind) query.set("kind", params.kind);
    if (params.category) query.set("category", params.category);
    if (params.sort) query.set("sort", params.sort);
    if (params.page) query.set("page", String(params.page));
    if (params.limit) query.set("limit", String(params.limit ?? 24));

    const res = await fetch(`${BASE}/marketplace/search?${query}`);
    if (!res.ok) throw new Error(`Search failed: ${res.status}`);
    return res.json();
  },

  async getPackage(name: string): Promise<PackageDetail> {
    // Encode the package name so slashes or reserved chars in scoped names
    // (`@scope/pkg`) do not break the URL path.
    const res = await fetch(
      `${BASE}/marketplace/packages/${encodeURIComponent(name)}`,
    );
    if (res.status === 404) {
      const e = new Error(`Package not found: ${name}`) as Error & {
        code?: string;
        status?: number;
      };
      e.code = "NOT_FOUND";
      e.status = 404;
      throw e;
    }
    if (!res.ok) {
      throw new Error(`Marketplace getPackage failed (${res.status})`);
    }
    return res.json();
  },

  async install(name: string, version?: string): Promise<void> {
    const res = await fetch(`${BASE}/marketplace/install`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ name, version }),
    });
    if (!res.ok) {
      const body = await res.text();
      throw new Error(`Install failed: ${body}`);
    }
  },

  async uninstall(name: string): Promise<void> {
    const res = await fetch(`${BASE}/marketplace/uninstall`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ name }),
    });
    if (!res.ok) throw new Error(`Uninstall failed: ${res.status}`);
  },

  async listInstalled(): Promise<MarketplacePackage[]> {
    const res = await fetch(`${BASE}/marketplace/installed`);
    if (!res.ok) throw new Error("Failed to list installed packages");
    return res.json();
  },

  async submitReview(name: string, rating: number, comment: string): Promise<void> {
    const res = await fetch(`${BASE}/marketplace/packages/${name}/reviews`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ rating, comment }),
    });
    if (!res.ok) throw new Error(`Review failed: ${res.status}`);
  },

  async getCategories(): Promise<string[]> {
    const res = await fetch(`${BASE}/marketplace/categories`);
    if (!res.ok) return [];
    return res.json();
  },
};
