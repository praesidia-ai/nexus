"use client";

import { useEffect, useRef, useState } from "react";
import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import {
  Search,
  LayoutGrid,
  List,
  Star,
  Zap,
  TrendingUp,
  Bot,
  Puzzle,
  Workflow,
  Wrench,
  Sparkles,
  Globe,
  Shield,
  Brain,
} from "lucide-react";
import { cn } from "@/lib/utils";
import { Input } from "@/components/ui/input";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { Badge } from "@/components/ui/badge";
import { PackageCard } from "@/components/marketplace/package-card";
import { ForgeTabs } from "@/components/marketplace/forge-tabs";
import { marketplaceApi, type PackageKind, type SearchParams } from "@/lib/marketplace-api";

const KINDS: Array<{ value: PackageKind | "all"; label: string; icon: React.ElementType }> = [
  { value: "all", label: "All", icon: Sparkles },
  { value: "agent", label: "Agents", icon: Bot },
  { value: "plugin", label: "Plugins", icon: Puzzle },
  { value: "workflow", label: "Workflows", icon: Workflow },
  { value: "tool", label: "Tools", icon: Wrench },
];

const SORTS: Array<{ value: SearchParams["sort"]; label: string }> = [
  { value: "downloads", label: "Popular" },
  { value: "rating", label: "Top Rated" },
  { value: "updated", label: "Recently Updated" },
  { value: "name", label: "Name" },
];

const FEATURED_COLLECTIONS = [
  {
    id: "ai-essentials",
    title: "AI Essentials",
    description: "Must-have agents for every project",
    icon: Star,
    color: "from-amber-500/20 to-orange-500/20",
    borderColor: "border-amber-500/20",
    iconColor: "text-amber-400",
    query: "essential",
  },
  {
    id: "security-compliance",
    title: "Security & Compliance",
    description: "Governance, PII detection, and audit tools",
    icon: Shield,
    color: "from-emerald-500/20 to-teal-500/20",
    borderColor: "border-emerald-500/20",
    iconColor: "text-emerald-400",
    query: "security",
  },
  {
    id: "automation",
    title: "Automation Power Pack",
    description: "Workflows, integrations, and scheduled tasks",
    icon: Zap,
    color: "from-violet-500/20 to-purple-500/20",
    borderColor: "border-violet-500/20",
    iconColor: "text-violet-400",
    query: "automation",
  },
  {
    id: "data-ai",
    title: "Data & Intelligence",
    description: "RAG pipelines, knowledge graphs, and analytics",
    icon: Brain,
    color: "from-blue-500/20 to-cyan-500/20",
    borderColor: "border-blue-500/20",
    iconColor: "text-blue-400",
    query: "data",
  },
  {
    id: "multi-agent",
    title: "Multi-Agent Teams",
    description: "Pre-built team configurations for complex tasks",
    icon: Globe,
    color: "from-pink-500/20 to-rose-500/20",
    borderColor: "border-pink-500/20",
    iconColor: "text-pink-400",
    query: "team",
  },
  {
    id: "trending",
    title: "Trending This Week",
    description: "Fastest growing packages by community installs",
    icon: TrendingUp,
    color: "from-red-500/20 to-orange-500/20",
    borderColor: "border-red-500/20",
    iconColor: "text-red-400",
    query: "trending",
  },
];

export default function MarketplacePage() {
  const qc = useQueryClient();
  const [q, setQ] = useState("");
  const [debouncedQ, setDebouncedQ] = useState("");
  const [kind, setKind] = useState<PackageKind | "all">("all");
  const [sort, setSort] = useState<SearchParams["sort"]>("downloads");
  const [page, setPage] = useState(1);
  const [view, setView] = useState<"grid" | "list">("grid");
  const [installingPkg, setInstallingPkg] = useState<string | null>(null);
  const [activeCollection, setActiveCollection] = useState<string | null>(null);

  const searchParams: SearchParams = {
    q: debouncedQ || undefined,
    kind: kind !== "all" ? kind : undefined,
    sort,
    page,
    limit: 24,
  };

  const { data, isLoading } = useQuery({
    queryKey: ["marketplace", searchParams],
    queryFn: () => marketplaceApi.search(searchParams),
    placeholderData: (prev) => prev,
  });

  useQuery({
    queryKey: ["marketplace-categories"],
    queryFn: () => marketplaceApi.getCategories(),
  });

  const installMutation = useMutation({
    mutationFn: (name: string) => marketplaceApi.install(name),
    onMutate: (name) => setInstallingPkg(name),
    onSettled: () => {
      setInstallingPkg(null);
      qc.invalidateQueries({ queryKey: ["marketplace"] });
    },
  });

  const uninstallMutation = useMutation({
    mutationFn: (name: string) => marketplaceApi.uninstall(name),
    onMutate: (name) => setInstallingPkg(name),
    onSettled: () => {
      setInstallingPkg(null);
      qc.invalidateQueries({ queryKey: ["marketplace"] });
    },
  });

  const searchDebounceRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  useEffect(() => {
    return () => {
      if (searchDebounceRef.current) clearTimeout(searchDebounceRef.current);
    };
  }, []);

  function handleSearch(value: string) {
    setQ(value);
    setActiveCollection(null);
    if (searchDebounceRef.current) clearTimeout(searchDebounceRef.current);
    searchDebounceRef.current = setTimeout(() => {
      setDebouncedQ(value);
      setPage(1);
    }, 300);
  }

  function handleCollectionClick(collection: (typeof FEATURED_COLLECTIONS)[number]) {
    setActiveCollection(collection.id);
    setQ(collection.query);
    setDebouncedQ(collection.query);
    setPage(1);
  }

  const packages = data?.packages ?? [];
  const showCollections = !debouncedQ && kind === "all" && page === 1;

  return (
    <div className="flex min-h-screen flex-col gap-6 p-6">
      {/* Hero */}
      <div className="relative overflow-hidden rounded-2xl bg-gradient-to-br from-violet-600/20 via-indigo-600/10 to-transparent border border-white/[0.06] p-8">
        <div className="relative z-10">
          <h1 className="text-3xl font-bold text-white">Marketplace</h1>
          <p className="text-sm text-white/50 mt-2 max-w-lg">
            Discover, install, and share community agents, plugins, workflows, and tools.
            One-click install to supercharge your NEXUS projects.
          </p>
        </div>
        <div className="absolute top-0 right-0 w-64 h-64 bg-gradient-to-bl from-violet-500/10 to-transparent rounded-full blur-3xl" />
      </div>

      {/* Search + controls */}
      <div className="flex flex-col gap-3 sm:flex-row sm:items-center">
        <div className="relative flex-1">
          <Search className="pointer-events-none absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-white/30" />
          <Input
            value={q}
            onChange={(e) => handleSearch(e.target.value)}
            placeholder="Search agents, plugins, workflows, tools..."
            className="pl-9 h-11"
          />
        </div>

        <div className="flex items-center gap-2">
          <select
            value={sort}
            onChange={(e) => { setSort(e.target.value as SearchParams["sort"]); setPage(1); }}
            className="rounded-lg border border-white/10 bg-white/5 px-3 py-2 text-sm text-white/70 focus:outline-none h-11"
          >
            {SORTS.map((s) => (
              <option key={s.value} value={s.value}>
                {s.label}
              </option>
            ))}
          </select>

          <div className="flex items-center rounded-lg border border-white/10 overflow-hidden h-11">
            <button
              onClick={() => setView("grid")}
              className={cn("p-2.5 transition-colors", view === "grid" ? "bg-white/10 text-white" : "text-white/30 hover:text-white/60")}
            >
              <LayoutGrid className="h-4 w-4" />
            </button>
            <button
              onClick={() => setView("list")}
              className={cn("p-2.5 transition-colors", view === "list" ? "bg-white/10 text-white" : "text-white/30 hover:text-white/60")}
            >
              <List className="h-4 w-4" />
            </button>
          </div>
        </div>
      </div>

      {/* Kind filter tabs */}
      <div className="flex gap-1.5 flex-wrap">
        {KINDS.map(({ value, label, icon: Icon }) => (
          <button
            key={value}
            onClick={() => { setKind(value); setPage(1); setActiveCollection(null); }}
            className={cn(
              "rounded-full px-4 py-1.5 text-xs font-medium transition-colors flex items-center gap-1.5",
              kind === value
                ? "bg-violet-600 text-white"
                : "bg-white/5 text-white/50 hover:bg-white/10 hover:text-white/80",
            )}
          >
            <Icon className="w-3.5 h-3.5" />
            {label}
          </button>
        ))}
      </div>

      {/* Forge community-reputation tabs (free-only, no payments).
          Renders above Featured Collections when the user hasn't
          typed a search query. */}
      {showCollections && <ForgeTabs />}

      {/* Featured Collections */}
      {showCollections && (
        <div>
          <h2 className="text-lg font-semibold text-white mb-3">Featured Collections</h2>
          <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-3">
            {FEATURED_COLLECTIONS.map((col) => (
              <button
                key={col.id}
                onClick={() => handleCollectionClick(col)}
                className={cn(
                  "text-left p-4 rounded-xl border transition-all hover:scale-[1.01]",
                  `bg-gradient-to-br ${col.color}`,
                  activeCollection === col.id ? "ring-1 ring-white/20" : col.borderColor,
                )}
              >
                <div className="flex items-center gap-3 mb-1.5">
                  <col.icon className={cn("w-5 h-5", col.iconColor)} />
                  <span className="text-sm font-medium text-white">{col.title}</span>
                </div>
                <p className="text-xs text-white/50">{col.description}</p>
              </button>
            ))}
          </div>
        </div>
      )}

      {/* Active collection badge */}
      {activeCollection && (
        <div className="flex items-center gap-2">
          <Badge className="bg-violet-600/30 text-violet-200 text-xs">
            Collection: {FEATURED_COLLECTIONS.find(c => c.id === activeCollection)?.title}
          </Badge>
          <button
            onClick={() => { setActiveCollection(null); setQ(""); setDebouncedQ(""); }}
            className="text-xs text-white/40 hover:text-white/70"
          >
            Clear
          </button>
        </div>
      )}

      {/* Results */}
      {isLoading ? (
        <div className={cn("grid gap-4", view === "grid" ? "grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4" : "grid-cols-1")}>
          {Array.from({ length: 12 }).map((_, i) => (
            <Skeleton key={i} className="h-44 rounded-xl" />
          ))}
        </div>
      ) : packages.length === 0 ? (
        <div className="flex flex-col items-center justify-center gap-3 py-24 text-white/30">
          <Search className="h-10 w-10" />
          <p className="text-sm">No packages found. Try a different search.</p>
        </div>
      ) : (
        <>
          <p className="text-xs text-white/30">
            {data?.total?.toLocaleString()} package{data?.total !== 1 ? "s" : ""} found
          </p>
          <div className={cn("grid gap-4", view === "grid" ? "grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4" : "grid-cols-1")}>
            {packages.map((pkg) => (
              <PackageCard
                key={pkg.name}
                pkg={pkg}
                onInstall={(n) => installMutation.mutate(n)}
                onUninstall={(n) => uninstallMutation.mutate(n)}
                installing={installingPkg === pkg.name}
              />
            ))}
          </div>

          {/* Pagination */}
          {data && data.totalPages > 1 && (
            <div className="flex items-center justify-center gap-2 pt-4">
              <Button
                variant="ghost"
                size="sm"
                disabled={page <= 1}
                onClick={() => setPage((p) => p - 1)}
              >
                Previous
              </Button>
              <span className="text-xs text-white/40">
                Page {page} of {data.totalPages}
              </span>
              <Button
                variant="ghost"
                size="sm"
                disabled={page >= data.totalPages}
                onClick={() => setPage((p) => p + 1)}
              >
                Next
              </Button>
            </div>
          )}
        </>
      )}
    </div>
  );
}
