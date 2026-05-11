"use client";

import { useState, useEffect, useCallback } from "react";
import { useParams } from "next/navigation";
import {
  Search,
  Brain,
  BookOpen,
  Sparkles,
  Clock,
  RefreshCw,
  Box,
  Puzzle,
  Lightbulb,
  FileText,
  Tag,
  TrendingUp,
  Zap,
  Database,
  Filter,
} from "lucide-react";
import { cn } from "@/lib/utils";
import { api, type KnowledgeItem } from "@/lib/api";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { StaggerItem } from "@/components/ui/stagger-list";

/* ------------------------------------------------------------------ */
/*  Constants                                                          */
/* ------------------------------------------------------------------ */

const KNOWLEDGE_TYPES = [
  { value: "all", label: "All Types", icon: Database },
  { value: "entity", label: "Entity", icon: Box },
  { value: "component", label: "Component", icon: Puzzle },
  { value: "pattern", label: "Pattern", icon: Sparkles },
  { value: "fact", label: "Fact", icon: Lightbulb },
] as const;

type KnowledgeTypeFilter = (typeof KNOWLEDGE_TYPES)[number]["value"];

const TYPE_STYLES: Record<string, { bg: string; text: string; icon: typeof Box }> = {
  entity: { bg: "bg-blue-500/10", text: "text-blue-400", icon: Box },
  component: { bg: "bg-violet-500/10", text: "text-violet-400", icon: Puzzle },
  pattern: { bg: "bg-amber-500/10", text: "text-amber-400", icon: Sparkles },
  fact: { bg: "bg-emerald-500/10", text: "text-emerald-400", icon: Lightbulb },
};

/* ------------------------------------------------------------------ */
/*  KnowledgeCard                                                      */
/* ------------------------------------------------------------------ */

function KnowledgeCard({ item, index }: { item: KnowledgeItem; index: number }) {
  const style = TYPE_STYLES[item.item_type] ?? {
    bg: "bg-slate-500/10",
    text: "text-slate-400",
    icon: FileText,
  };
  const IconComponent = style.icon;

  return (
    <StaggerItem index={index}>
      <Card className="border-white/[0.08] bg-white/[0.02] hover:bg-white/[0.04] transition-colors group">
        <CardContent className="p-4">
          <div className="flex items-start gap-3">
            <div
              className={cn(
                "rounded-lg p-2 shrink-0 transition-transform group-hover:scale-105",
                style.bg
              )}
            >
              <IconComponent className={cn("h-4 w-4", style.text)} />
            </div>
            <div className="min-w-0 flex-1">
              <div className="flex items-center gap-2 mb-1">
                <h3 className="text-sm font-medium text-slate-200 truncate">
                  {item.icon ? `${item.icon} ` : ""}
                  {item.name}
                </h3>
                <Badge variant="outline" className="text-[10px] shrink-0">
                  {item.item_type}
                </Badge>
              </div>
              {item.description && (
                <p className="text-xs text-slate-400 leading-relaxed line-clamp-2 mb-2">
                  {item.description}
                </p>
              )}
              <div className="flex items-center gap-2 text-[10px] text-slate-500">
                <Clock className="h-2.5 w-2.5" />
                {new Date(item.created_at).toLocaleString()}
              </div>
            </div>
          </div>
        </CardContent>
      </Card>
    </StaggerItem>
  );
}

/* ------------------------------------------------------------------ */
/*  LearningInsights                                                   */
/* ------------------------------------------------------------------ */

interface MemoryKnowledgeItem {
  key: string;
  value: string;
  category: string;
  confidence: number;
}

function LearningInsights({
  items,
  loading,
}: {
  items: MemoryKnowledgeItem[];
  loading: boolean;
}) {
  if (loading) {
    return (
      <Card className="border-white/[0.08] bg-white/[0.02]">
        <CardHeader>
          <CardTitle className="text-sm font-medium text-slate-200 flex items-center gap-2">
            <TrendingUp className="w-4 h-4 text-slate-400" />
            Learning Insights
          </CardTitle>
        </CardHeader>
        <CardContent className="space-y-3">
          {[1, 2, 3].map((i) => (
            <div key={i} className="space-y-1.5">
              <Skeleton className="h-4 w-40" />
              <Skeleton className="h-3 w-full" />
            </div>
          ))}
        </CardContent>
      </Card>
    );
  }

  if (items.length === 0) {
    return (
      <Card className="border-white/[0.08] bg-white/[0.02]">
        <CardHeader>
          <CardTitle className="text-sm font-medium text-slate-200 flex items-center gap-2">
            <TrendingUp className="w-4 h-4 text-slate-400" />
            Learning Insights
          </CardTitle>
        </CardHeader>
        <CardContent>
          <div className="py-6 text-center">
            <Brain className="w-8 h-8 text-slate-600 mx-auto mb-2" />
            <p className="text-sm text-slate-400">No learning insights yet</p>
            <p className="text-xs text-slate-500 mt-1">
              The system will learn from your interactions over time.
            </p>
          </div>
        </CardContent>
      </Card>
    );
  }

  return (
    <Card className="border-white/[0.08] bg-white/[0.02]">
      <CardHeader>
        <CardTitle className="text-sm font-medium text-slate-200 flex items-center gap-2">
          <TrendingUp className="w-4 h-4 text-slate-400" />
          Learning Insights
          <Badge variant="outline" className="text-[10px] ml-auto">
            {items.length}
          </Badge>
        </CardTitle>
      </CardHeader>
      <CardContent className="space-y-2">
        {items.map((item, i) => (
          <div
            key={item.key}
            className="opacity-0 animate-fade-in-up flex items-start gap-3 py-2 border-b border-white/[0.04] last:border-0"
            style={{ animationDelay: `${i * 50}ms` }}
          >
            <div className="rounded-lg bg-cyan-500/10 p-1.5 shrink-0 mt-0.5">
              <Zap className="h-3 w-3 text-cyan-400" />
            </div>
            <div className="min-w-0 flex-1">
              <div className="flex items-center gap-2 mb-0.5">
                <span className="text-xs font-medium text-slate-200">{item.key}</span>
                <Badge variant="outline" className="text-[9px]">
                  {item.category}
                </Badge>
              </div>
              <p className="text-[11px] text-slate-400 leading-relaxed">{item.value}</p>
            </div>
            <div className="shrink-0">
              <div
                className={cn(
                  "text-[10px] font-medium px-1.5 py-0.5 rounded",
                  item.confidence >= 0.8
                    ? "bg-emerald-500/10 text-emerald-400"
                    : item.confidence >= 0.5
                      ? "bg-amber-500/10 text-amber-400"
                      : "bg-slate-500/10 text-slate-400"
                )}
              >
                {Math.round(item.confidence * 100)}%
              </div>
            </div>
          </div>
        ))}
      </CardContent>
    </Card>
  );
}

/* ------------------------------------------------------------------ */
/*  MemoryExplorer                                                     */
/* ------------------------------------------------------------------ */

interface Episode {
  id: string;
  content: string;
  tags: string[];
  timestamp: string;
}

function MemoryExplorer({
  episodes,
  loading,
}: {
  episodes: Episode[];
  loading: boolean;
}) {
  if (loading) {
    return (
      <Card className="border-white/[0.08] bg-white/[0.02]">
        <CardHeader>
          <CardTitle className="text-sm font-medium text-slate-200 flex items-center gap-2">
            <BookOpen className="w-4 h-4 text-slate-400" />
            Memory Explorer
          </CardTitle>
        </CardHeader>
        <CardContent className="space-y-3">
          {[1, 2, 3].map((i) => (
            <div key={i} className="space-y-1.5">
              <Skeleton className="h-4 w-full" />
              <Skeleton className="h-3 w-3/4" />
              <Skeleton className="h-3 w-32" />
            </div>
          ))}
        </CardContent>
      </Card>
    );
  }

  if (episodes.length === 0) {
    return (
      <Card className="border-white/[0.08] bg-white/[0.02]">
        <CardHeader>
          <CardTitle className="text-sm font-medium text-slate-200 flex items-center gap-2">
            <BookOpen className="w-4 h-4 text-slate-400" />
            Memory Explorer
          </CardTitle>
        </CardHeader>
        <CardContent>
          <div className="py-6 text-center">
            <BookOpen className="w-8 h-8 text-slate-600 mx-auto mb-2" />
            <p className="text-sm text-slate-400">No episodic memories yet</p>
            <p className="text-xs text-slate-500 mt-1">
              Memories are created as agents work on tasks.
            </p>
          </div>
        </CardContent>
      </Card>
    );
  }

  return (
    <Card className="border-white/[0.08] bg-white/[0.02]">
      <CardHeader>
        <CardTitle className="text-sm font-medium text-slate-200 flex items-center gap-2">
          <BookOpen className="w-4 h-4 text-slate-400" />
          Memory Explorer
          <Badge variant="outline" className="text-[10px] ml-auto">
            {episodes.length} episodes
          </Badge>
        </CardTitle>
      </CardHeader>
      <CardContent className="space-y-2 max-h-[500px] overflow-y-auto">
        {episodes.map((episode, i) => (
          <div
            key={episode.id}
            className="opacity-0 animate-fade-in-up py-3 border-b border-white/[0.04] last:border-0"
            style={{ animationDelay: `${i * 40}ms` }}
          >
            <p className="text-xs text-slate-200 leading-relaxed mb-2 whitespace-pre-wrap">
              {episode.content}
            </p>
            <div className="flex flex-wrap items-center gap-2">
              {(episode.tags ?? []).map((tag) => (
                <Badge key={tag} variant="outline" className="text-[10px]">
                  <Tag className="h-2.5 w-2.5 mr-1" />
                  {tag}
                </Badge>
              ))}
              <span className="text-[10px] text-slate-500 flex items-center gap-1">
                <Clock className="h-2.5 w-2.5" />
                {new Date(episode.timestamp).toLocaleString()}
              </span>
            </div>
          </div>
        ))}
      </CardContent>
    </Card>
  );
}

/* ------------------------------------------------------------------ */
/*  Page                                                               */
/* ------------------------------------------------------------------ */

export default function KnowledgePage() {
  const params = useParams();
  const projectId = params.projectId as string;

  // Knowledge items state
  const [items, setItems] = useState<KnowledgeItem[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [searchQuery, setSearchQuery] = useState("");
  const [typeFilter, setTypeFilter] = useState<KnowledgeTypeFilter>("all");

  // Learning insights state
  const [insights, setInsights] = useState<MemoryKnowledgeItem[]>([]);
  const [insightsLoading, setInsightsLoading] = useState(true);

  // Memory explorer state
  const [episodes, setEpisodes] = useState<Episode[]>([]);
  const [episodesLoading, setEpisodesLoading] = useState(true);

  // Active tab
  const [activeTab, setActiveTab] = useState<"items" | "insights" | "memory">("items");

  const fetchKnowledge = useCallback(async () => {
    try {
      const data = await api.listKnowledge(projectId);
      setItems(Array.isArray(data) ? data : []);
      setError(null);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to load knowledge");
    } finally {
      setLoading(false);
    }
  }, [projectId]);

  const fetchInsights = useCallback(async () => {
    try {
      const data = await api.getMemoryKnowledge();
      setInsights(data?.items ?? []);
    } catch {
      setInsights([]);
    } finally {
      setInsightsLoading(false);
    }
  }, []);

  const fetchEpisodes = useCallback(async () => {
    try {
      const data = await api.getMemoryEpisodes(projectId);
      setEpisodes(data?.episodes ?? []);
    } catch {
      setEpisodes([]);
    } finally {
      setEpisodesLoading(false);
    }
  }, [projectId]);

  useEffect(() => {
    fetchKnowledge();
    fetchInsights();
    fetchEpisodes();
  }, [fetchKnowledge, fetchInsights, fetchEpisodes]);

  const handleRefresh = () => {
    setLoading(true);
    setInsightsLoading(true);
    setEpisodesLoading(true);
    fetchKnowledge();
    fetchInsights();
    fetchEpisodes();
  };

  // Filter and search
  const filteredItems = items.filter((item) => {
    const q = searchQuery.trim().toLowerCase();
    const matchesType = typeFilter === "all" || item.item_type === typeFilter;
    const matchesSearch =
      !q ||
      (item.name ?? "").toLowerCase().includes(q) ||
      (item.description ?? "").toLowerCase().includes(q) ||
      (item.item_type ?? "").toLowerCase().includes(q);
    return matchesType && matchesSearch;
  });

  // Stats
  const typeCounts = items.reduce<Record<string, number>>((acc, item) => {
    acc[item.item_type] = (acc[item.item_type] ?? 0) + 1;
    return acc;
  }, {});

  const tabs = [
    { id: "items" as const, label: "Knowledge Items", icon: Database, count: items.length },
    { id: "insights" as const, label: "Learning Insights", icon: TrendingUp, count: insights.length },
    { id: "memory" as const, label: "Memory Explorer", icon: BookOpen, count: episodes.length },
  ];

  return (
    <div className="flex flex-col gap-6 p-6 overflow-auto h-full">
      {/* Header */}
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-3">
          <div className="w-9 h-9 rounded-lg bg-gradient-to-br from-cyan-500/20 to-blue-500/20 border border-cyan-500/10 flex items-center justify-center">
            <Brain className="w-4 h-4 text-cyan-400" />
          </div>
          <div>
            <h1 className="text-lg font-semibold text-slate-200">Knowledge Browser</h1>
            <p className="text-xs text-slate-400">
              {loading
                ? "Loading..."
                : `${items.length} item${items.length !== 1 ? "s" : ""} discovered`}
            </p>
          </div>
        </div>
        <Button
          variant="ghost"
          size="sm"
          className="text-slate-400 hover:text-slate-200"
          onClick={handleRefresh}
        >
          <RefreshCw
            className={cn("h-4 w-4 mr-1.5", loading ? "animate-spin" : "")}
          />
          Refresh
        </Button>
      </div>

      {/* Stat cards */}
      <div className="grid grid-cols-2 sm:grid-cols-4 gap-3">
        {KNOWLEDGE_TYPES.filter((t) => t.value !== "all").map((type) => {
          const count = typeCounts[type.value] ?? 0;
          const style = TYPE_STYLES[type.value];
          const Icon = type.icon;
          return (
            <button
              key={type.value}
              onClick={() =>
                setTypeFilter((prev) =>
                  prev === type.value ? "all" : type.value as KnowledgeTypeFilter
                )
              }
              className={cn(
                "rounded-lg border p-3 text-left transition-all",
                typeFilter === type.value
                  ? "border-white/[0.15] bg-white/[0.05]"
                  : "border-white/[0.06] bg-white/[0.02] hover:bg-white/[0.04]"
              )}
            >
              <div className="flex items-center gap-2 mb-1">
                <div className={cn("rounded p-1", style?.bg)}>
                  <Icon className={cn("h-3 w-3", style?.text)} />
                </div>
                <span className="text-[10px] text-slate-500 uppercase tracking-wider">
                  {type.label}
                </span>
              </div>
              <p className="text-lg font-semibold text-slate-200">{count}</p>
            </button>
          );
        })}
      </div>

      {/* Tabs */}
      <div className="flex items-center gap-1 border-b border-white/[0.06] pb-px">
        {tabs.map((tab) => {
          const Icon = tab.icon;
          return (
            <button
              key={tab.id}
              onClick={() => setActiveTab(tab.id)}
              className={cn(
                "flex items-center gap-1.5 px-3 py-2 text-xs font-medium rounded-t-lg transition-colors",
                activeTab === tab.id
                  ? "text-cyan-400 bg-white/[0.04] border-b-2 border-cyan-400"
                  : "text-slate-500 hover:text-slate-300"
              )}
            >
              <Icon className="h-3.5 w-3.5" />
              {tab.label}
              <span
                className={cn(
                  "text-[10px] px-1.5 py-0.5 rounded-full",
                  activeTab === tab.id
                    ? "bg-cyan-500/10 text-cyan-400"
                    : "bg-white/[0.05] text-slate-500"
                )}
              >
                {tab.count}
              </span>
            </button>
          );
        })}
      </div>

      {/* Error */}
      {error && (
        <div className="rounded-lg border border-red-500/20 bg-red-500/5 p-4 text-sm text-red-400">
          {error}
        </div>
      )}

      {/* Tab: Knowledge Items */}
      {activeTab === "items" && (
        <>
          {/* Search and filter bar */}
          <div className="flex items-center gap-3">
            <div className="relative flex-1">
              <Search className="absolute left-3 top-1/2 -translate-y-1/2 h-4 w-4 text-slate-500" />
              <input
                type="text"
                value={searchQuery}
                onChange={(e) => setSearchQuery(e.target.value)}
                placeholder="Search knowledge items..."
                className="w-full rounded-lg border border-white/[0.08] bg-white/[0.03] pl-10 pr-4 py-2.5 text-sm text-slate-200 placeholder:text-slate-500 focus:outline-none focus:border-glow-cyan/30 focus:shadow-glow-sm"
              />
            </div>
            <div className="flex items-center gap-1">
              <Filter className="h-3.5 w-3.5 text-slate-500" />
              {KNOWLEDGE_TYPES.map((type) => (
                <button
                  key={type.value}
                  onClick={() => setTypeFilter(type.value)}
                  className={cn(
                    "px-2.5 py-1.5 rounded-md text-[11px] font-medium transition-colors",
                    typeFilter === type.value
                      ? "bg-white/[0.08] text-slate-200"
                      : "text-slate-500 hover:text-slate-300"
                  )}
                >
                  {type.label}
                </button>
              ))}
            </div>
          </div>

          {/* Knowledge grid */}
          {loading ? (
            <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
              {[1, 2, 3, 4, 5, 6].map((i) => (
                <Card key={i} className="border-white/[0.08] bg-white/[0.02]">
                  <CardContent className="p-4">
                    <div className="flex items-start gap-3">
                      <Skeleton className="h-8 w-8 rounded-lg" />
                      <div className="flex-1 space-y-2">
                        <Skeleton className="h-4 w-32" />
                        <Skeleton className="h-3 w-full" />
                        <Skeleton className="h-3 w-24" />
                      </div>
                    </div>
                  </CardContent>
                </Card>
              ))}
            </div>
          ) : filteredItems.length > 0 ? (
            <>
              <p className="text-xs text-slate-500">
                {filteredItems.length} result{filteredItems.length !== 1 ? "s" : ""}
                {typeFilter !== "all" && ` (filtered by ${typeFilter})`}
              </p>
              <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
                {filteredItems.map((item, i) => (
                  <KnowledgeCard key={item.id} item={item} index={i} />
                ))}
              </div>
            </>
          ) : (
            <div className="flex flex-col items-center justify-center py-16 text-slate-500">
              <Database className="h-10 w-10 mb-3 opacity-40" />
              <p className="text-sm font-medium">No knowledge items found</p>
              <p className="text-xs mt-1">
                {searchQuery || typeFilter !== "all"
                  ? "Try adjusting your search or filter criteria."
                  : "Knowledge items are discovered as the system analyzes your project."}
              </p>
            </div>
          )}
        </>
      )}

      {/* Tab: Learning Insights */}
      {activeTab === "insights" && (
        <LearningInsights items={insights} loading={insightsLoading} />
      )}

      {/* Tab: Memory Explorer */}
      {activeTab === "memory" && (
        <MemoryExplorer episodes={episodes} loading={episodesLoading} />
      )}
    </div>
  );
}
