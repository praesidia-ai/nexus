"use client";

import { useState, useCallback } from "react";
import { useParams } from "next/navigation";
import {
  Search,
  Plus,
  Database,
  Tag,
  Clock,
  Loader2,
  X,
  Send,
} from "lucide-react";
import { cn } from "@/lib/utils";
import { api, type MemoryEntry } from "@/lib/api";
import { useToast } from "@/components/toast";
import { PageHeader } from "@/components/page-header";
import { Card, CardContent } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";

/* ------------------------------------------------------------------ */
/*  MemoryCard                                                         */
/* ------------------------------------------------------------------ */

function MemoryCard({ entry }: { entry: MemoryEntry }) {
  const relevancePercent = entry.relevance != null ? Math.round(entry.relevance * 100) : null;

  return (
    <Card className="border-white/[0.08] bg-white/[0.02] hover:bg-white/[0.04] transition-colors">
      <CardContent className="p-4">
        <div className="flex items-start justify-between gap-3">
          <div className="min-w-0 flex-1">
            <p className="text-sm text-slate-200 leading-relaxed mb-2 whitespace-pre-wrap">
              {entry.content}
            </p>
            <div className="flex flex-wrap items-center gap-2">
              {entry.tags.map((tag) => (
                <Badge key={tag} variant="outline" className="text-[10px]">
                  <Tag className="h-2.5 w-2.5 mr-1" />
                  {tag}
                </Badge>
              ))}
              <span className="text-[10px] text-slate-500 flex items-center gap-1">
                <Clock className="h-2.5 w-2.5" />
                {new Date(entry.timestamp).toLocaleString()}
              </span>
            </div>
          </div>
          {relevancePercent != null && (
            <div className="shrink-0 flex flex-col items-center">
              <div
                className={cn(
                  "rounded-full h-10 w-10 flex items-center justify-center text-xs font-semibold border",
                  relevancePercent >= 80
                    ? "border-emerald-500/30 text-emerald-400 bg-emerald-500/10"
                    : relevancePercent >= 50
                      ? "border-amber-500/30 text-amber-400 bg-amber-500/10"
                      : "border-slate-500/30 text-slate-400 bg-white/[0.03]"
                )}
              >
                {relevancePercent}%
              </div>
              <span className="text-[9px] text-slate-500 mt-0.5">match</span>
            </div>
          )}
        </div>
      </CardContent>
    </Card>
  );
}

/* ------------------------------------------------------------------ */
/*  Store memory form                                                  */
/* ------------------------------------------------------------------ */

function StoreMemoryForm({
  projectId,
  onStored,
}: {
  projectId: string;
  onStored: (entry: MemoryEntry) => void;
}) {
  const { toast } = useToast();
  const [open, setOpen] = useState(false);
  const [content, setContent] = useState("");
  const [tagsInput, setTagsInput] = useState("");
  const [storing, setStoring] = useState(false);

  const handleStore = async () => {
    if (!content.trim()) return;
    setStoring(true);
    try {
      const tags = tagsInput
        .split(",")
        .map((t) => t.trim())
        .filter(Boolean);
      const entry = await api.storeMemory(projectId, content.trim(), tags);
      onStored(entry);
      toast("success", "Memory stored");
      setContent("");
      setTagsInput("");
      setOpen(false);
    } catch (err) {
      const msg = err instanceof Error ? err.message : "Failed to store memory";
      toast("info", "Manual storage unavailable", msg);
    } finally {
      setStoring(false);
    }
  };

  if (!open) {
    return (
      <Button
        variant="ghost"
        size="sm"
        className="text-glow-cyan hover:text-glow-cyan/80"
        onClick={() => setOpen(true)}
      >
        <Plus className="h-4 w-4 mr-1.5" />
        Store Memory
      </Button>
    );
  }

  return (
    <Card className="border-white/[0.08] bg-white/[0.02]">
      <CardContent className="p-4 space-y-3">
        <div className="flex items-center justify-between">
          <span className="text-sm font-medium">Store New Memory</span>
          <Button
            variant="ghost"
            size="sm"
            className="h-6 w-6 p-0 text-slate-500"
            onClick={() => setOpen(false)}
          >
            <X className="h-3.5 w-3.5" />
          </Button>
        </div>
        <textarea
          value={content}
          onChange={(e) => setContent(e.target.value)}
          placeholder="Memory content..."
          rows={3}
          className="w-full rounded-lg border border-white/[0.08] bg-white/[0.03] px-3 py-2 text-sm text-slate-200 placeholder:text-slate-500 focus:outline-none focus:border-glow-cyan/30 focus:shadow-glow-sm resize-none"
        />
        <input
          type="text"
          value={tagsInput}
          onChange={(e) => setTagsInput(e.target.value)}
          placeholder="Tags (comma-separated)..."
          aria-label="Memory tags (comma-separated)"
          className="w-full rounded-lg border border-white/[0.08] bg-white/[0.03] px-3 py-2 text-sm text-slate-200 placeholder:text-slate-500 focus:outline-none focus:border-glow-cyan/30 focus:shadow-glow-sm"
        />
        <div className="flex justify-end">
          <Button
            size="sm"
            className="bg-glow-cyan/10 text-glow-cyan hover:bg-glow-cyan/20"
            onClick={handleStore}
            disabled={storing || !content.trim()}
          >
            {storing ? (
              <Loader2 className="h-3.5 w-3.5 animate-spin mr-1.5" />
            ) : (
              <Send className="h-3.5 w-3.5 mr-1.5" />
            )}
            Store
          </Button>
        </div>
      </CardContent>
    </Card>
  );
}

/* ------------------------------------------------------------------ */
/*  Page                                                               */
/* ------------------------------------------------------------------ */

export default function MemoryPage() {
  const params = useParams();
  const projectId = params.projectId as string;
  const { toast } = useToast();

  const [query, setQuery] = useState("");
  const [entries, setEntries] = useState<MemoryEntry[]>([]);
  const [loading, setLoading] = useState(false);
  const [searched, setSearched] = useState(false);

  const handleSearch = useCallback(async () => {
    setLoading(true);
    setSearched(true);
    try {
      const data = await api.recallMemory(projectId, query.trim());
      setEntries(data);
    } catch (err) {
      setEntries([]);
      const msg = err instanceof Error ? err.message : "Failed to search memory";
      toast("error", "Memory search failed", msg);
    } finally {
      setLoading(false);
    }
  }, [projectId, query, toast]);

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Enter") {
      e.preventDefault();
      handleSearch();
    }
  };

  return (
    <div className="flex flex-col gap-6 p-6">
      <PageHeader
        title="Memory Browser"
        subtitle="Search and store project memory entries"
        actions={
          <StoreMemoryForm
            projectId={projectId}
            onStored={(entry) => setEntries((prev) => [entry, ...prev])}
          />
        }
      />

      {/* Search bar */}
      <div className="relative">
        <Search className="absolute left-3 top-1/2 -translate-y-1/2 h-4 w-4 text-slate-500" />
        <input
          type="text"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          onKeyDown={handleKeyDown}
          placeholder="Search memories..."
          aria-label="Search memories"
          className="w-full rounded-lg border border-white/[0.08] bg-white/[0.03] pl-10 pr-24 py-2.5 text-sm text-slate-200 placeholder:text-slate-500 focus:outline-none focus:border-glow-cyan/30 focus:shadow-glow-sm"
        />
        <Button
          size="sm"
          className="absolute right-1.5 top-1/2 -translate-y-1/2 bg-glow-cyan/10 text-glow-cyan hover:bg-glow-cyan/20"
          onClick={handleSearch}
          disabled={loading || !query.trim()}
        >
          {loading ? (
            <Loader2 className="h-3.5 w-3.5 animate-spin mr-1.5" />
          ) : (
            <Search className="h-3.5 w-3.5 mr-1.5" />
          )}
          Recall
        </Button>
      </div>

      {/* Results */}
      <div className="flex flex-col gap-2">
        {loading ? (
          <>
            {[1, 2, 3].map((i) => (
              <Card key={i} className="border-white/[0.08] bg-white/[0.02]">
                <CardContent className="p-4 space-y-2">
                  <Skeleton className="h-4 w-full" />
                  <Skeleton className="h-4 w-3/4" />
                  <Skeleton className="h-3 w-40" />
                </CardContent>
              </Card>
            ))}
          </>
        ) : entries.length > 0 ? (
          <>
            <p className="text-xs text-slate-500">
              {entries.length} result{entries.length !== 1 ? "s" : ""}
            </p>
            {entries.map((entry) => (
              <MemoryCard key={entry.id} entry={entry} />
            ))}
          </>
        ) : searched ? (
          <div className="flex flex-col items-center justify-center py-16 text-slate-500">
            <Database className="h-10 w-10 mb-3 opacity-40" />
            <p className="text-sm font-medium">No memories found</p>
            <p className="text-xs mt-1">
              Try a different query or store new memories.
            </p>
          </div>
        ) : (
          <div className="flex flex-col items-center justify-center py-16 text-slate-500">
            <Search className="h-10 w-10 mb-3 opacity-40" />
            <p className="text-sm font-medium">Search project memory</p>
            <p className="text-xs mt-1">
              Enter a query above to recall relevant memories.
            </p>
          </div>
        )}
      </div>
    </div>
  );
}
