"use client";

import { useState, useEffect, useCallback } from "react";
import { useParams } from "next/navigation";
import {
  Globe,
  Copy,
  Check,
  ExternalLink,
  Loader2,
  RefreshCw,
  Rocket,
} from "lucide-react";
import { Card, CardHeader, CardTitle, CardContent, CardDescription } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { useToast } from "@/components/toast";
import { api, type Portal } from "@/lib/api";
import { safeHttpUrl, safeCopyToClipboard } from "@/lib/utils";;

// ---------------------------------------------------------------------------
// Types — matches backend Portal shape (see portal.rs portal_view)
// ---------------------------------------------------------------------------

interface PortalConfig extends Portal {
  /** Derived client-side from `published_at`. */
  readonly _published?: boolean;
}

function portalPublishedUrl(p: Portal | null): string | undefined {
  if (!p?.slug) return undefined;
  if (typeof window === "undefined") return undefined;
  return `${window.location.origin}/portal/${encodeURIComponent(p.slug)}`;
}

// ---------------------------------------------------------------------------
// Page
// ---------------------------------------------------------------------------

export default function PortalPage() {
  const params = useParams();
  const projectId = params.projectId as string;
  const { toast } = useToast();

  const [portal, setPortal] = useState<PortalConfig | null>(null);
  const [projectName, setProjectName] = useState<string>("");
  const [agentName, setAgentName] = useState<string>("Agent");
  const [slug, setSlug] = useState<string>("");
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [publishing, setPublishing] = useState(false);
  const [copied, setCopied] = useState(false);

  const fetchPortal = useCallback(async () => {
    try {
      const data = await api.getPortal(projectId);
      setPortal(data as PortalConfig);
      if (data?.project_name && !projectName) setProjectName(data.project_name);
      if (data?.agent_name && !agentName) setAgentName(data.agent_name);
      if (data?.slug && !slug) setSlug(data.slug);
      setError(null);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to load portal");
    } finally {
      setLoading(false);
    }
  }, [projectId, projectName, agentName, slug]);

  useEffect(() => {
    fetchPortal();
  }, [fetchPortal]);

  const publishedUrl = portalPublishedUrl(portal);
  const isPublished = Boolean(portal?.published_at);

  const handlePublish = async () => {
    setPublishing(true);
    try {
      await api.publishPortal(projectId, {
        project_name: projectName.trim() || "Untitled Project",
        agent_name: agentName.trim() || "Agent",
        ...(slug.trim() ? { slug: slug.trim() } : {}),
      });
      toast("success", "Portal published");
      await fetchPortal();
    } catch (err) {
      const msg = err instanceof Error ? err.message : "Publish failed";
      setError(msg);
      toast("error", "Publish failed", msg);
    } finally {
      setPublishing(false);
    }
  };

  const handleCopyUrl = () => {
    if (publishedUrl) {
      void safeCopyToClipboard(publishedUrl);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    }
  };

  if (loading) {
    return (
      <div className="p-6 space-y-6">
        <div className="flex items-center gap-3">
          <Skeleton className="h-8 w-48" />
          <Skeleton className="h-8 w-24" />
        </div>
        <Skeleton className="h-40 rounded-xl" />
        <Skeleton className="h-[400px] rounded-xl" />
      </div>
    );
  }

  return (
    <div className="flex flex-col gap-6 p-6 overflow-auto h-full">
      {/* Header */}
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-3">
          <div className="w-9 h-9 rounded-lg bg-gradient-to-br from-emerald-500/20 to-cyan-500/20 border border-emerald-500/10 flex items-center justify-center">
            <Globe className="w-4 h-4 text-emerald-400" />
          </div>
          <div>
            <h1 className="text-lg font-semibold text-slate-200">Portal</h1>
            <p className="text-xs text-slate-400">Publish and manage your project portal</p>
          </div>
        </div>
        <Button
          variant="ghost"
          size="sm"
          className="text-slate-400 hover:text-slate-200"
          onClick={() => {
            setLoading(true);
            fetchPortal();
          }}
        >
          <RefreshCw className="h-4 w-4 mr-1.5" />
          Refresh
        </Button>
      </div>

      {/* Error */}
      {error && (
        <div className="rounded-lg border border-red-500/20 bg-red-500/5 p-4 text-sm text-red-400">
          {error}
        </div>
      )}

      {/* Portal Config */}
      <Card className="border-white/[0.08] bg-white/[0.02]">
        <CardHeader>
          <div className="flex items-center justify-between">
            <div>
              <CardTitle className="text-sm font-medium text-slate-200">Configuration</CardTitle>
              <CardDescription className="text-xs mt-1">
                Portal settings and publish status
              </CardDescription>
            </div>
            <Badge
              variant={isPublished ? "success" : "secondary"}
              className="text-[10px]"
            >
              {isPublished ? "Published" : "Draft"}
            </Badge>
          </div>
        </CardHeader>
        <CardContent className="space-y-4">
          {/* Editable name + agent + slug */}
          <div className="grid grid-cols-1 sm:grid-cols-2 gap-4">
            <div>
              <label className="text-xs text-slate-500 block mb-1">Project name</label>
              <input
                className="w-full rounded-lg border border-white/[0.08] bg-white/[0.03] px-3 py-2 text-sm text-slate-200 focus:outline-none focus:ring-1 focus:ring-glow-cyan/40"
                placeholder="My SaaS"
                value={projectName}
                onChange={(e) => setProjectName(e.target.value)}
              />
            </div>
            <div>
              <label className="text-xs text-slate-500 block mb-1">Agent name</label>
              <input
                className="w-full rounded-lg border border-white/[0.08] bg-white/[0.03] px-3 py-2 text-sm text-slate-200 focus:outline-none focus:ring-1 focus:ring-glow-cyan/40"
                placeholder="Agent"
                value={agentName}
                onChange={(e) => setAgentName(e.target.value)}
              />
            </div>
            <div className="sm:col-span-2">
              <label className="text-xs text-slate-500 block mb-1">Slug (optional)</label>
              <input
                className="w-full rounded-lg border border-white/[0.08] bg-white/[0.03] px-3 py-2 text-sm text-slate-200 focus:outline-none focus:ring-1 focus:ring-glow-cyan/40"
                placeholder="my-saas (auto-derived from name if blank)"
                value={slug}
                onChange={(e) => setSlug(e.target.value)}
              />
            </div>
          </div>

          {/* Published URL */}
          {publishedUrl && (
            <div>
              <label className="text-xs text-slate-500 block mb-1">Published URL</label>
              <div className="flex items-center gap-2">
                <div className="flex-1 rounded-lg border border-white/[0.08] bg-white/[0.03] px-3 py-2 text-sm text-glow-cyan truncate">
                  {publishedUrl}
                </div>
                <Button
                  variant="outline"
                  size="sm"
                  className="shrink-0"
                  onClick={handleCopyUrl}
                >
                  {copied ? (
                    <Check className="h-3.5 w-3.5 text-emerald-400" />
                  ) : (
                    <Copy className="h-3.5 w-3.5" />
                  )}
                </Button>
                <a
                  href={publishedUrl}
                  target="_blank"
                  rel="noopener noreferrer"
                >
                  <Button variant="outline" size="sm" className="shrink-0">
                    <ExternalLink className="h-3.5 w-3.5" />
                  </Button>
                </a>
              </div>
            </div>
          )}

          {/* Last published */}
          {portal?.published_at && (
            <p className="text-[10px] text-slate-500">
              Last published: {new Date(portal.published_at).toLocaleString()}
            </p>
          )}

          {/* Publish button */}
          <div className="flex items-center gap-2 pt-2">
            <Button
              size="sm"
              className="bg-glow-cyan/10 text-glow-cyan hover:bg-glow-cyan/20"
              onClick={handlePublish}
              disabled={publishing}
            >
              {publishing ? (
                <Loader2 className="h-3.5 w-3.5 animate-spin mr-1.5" />
              ) : (
                <Rocket className="h-3.5 w-3.5 mr-1.5" />
              )}
              {isPublished ? "Republish" : "Publish"}
            </Button>
          </div>
        </CardContent>
      </Card>

      {/* Preview */}
      {isPublished && publishedUrl && (
        <Card className="border-white/[0.08] bg-white/[0.02]">
          <CardHeader>
            <CardTitle className="text-sm font-medium text-slate-200">Preview</CardTitle>
          </CardHeader>
          <CardContent>
            <div className="rounded-lg border border-white/[0.08] overflow-hidden bg-white">
              <iframe
                src={safeHttpUrl(publishedUrl) ?? "about:blank"}
                className="w-full h-[500px] border-0"
                title="Portal preview"
                sandbox="allow-scripts allow-same-origin"
              />
            </div>
          </CardContent>
        </Card>
      )}
    </div>
  );
}
