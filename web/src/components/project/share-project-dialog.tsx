"use client";

import { useEffect, useState } from "react";
import { Copy, Check, ExternalLink, Globe, Loader2, Rocket } from "lucide-react";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { api, BASE, type Portal, type Project } from "@/lib/api";
import { useToast } from "@/components/toast";
import { safeHttpUrl, safeCopyToClipboard } from "@/lib/utils";

interface ShareProjectDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  project: Project | null | undefined;
}

export function ShareProjectDialog({ open, onOpenChange, project }: ShareProjectDialogProps) {
  const { toast } = useToast();
  const [portal, setPortal] = useState<Portal | null>(null);
  const [loading, setLoading] = useState(false);
  const [publishing, setPublishing] = useState(false);
  const [copied, setCopied] = useState(false);

  useEffect(() => {
    if (!open || !project?.id) return;
    setLoading(true);
    setPortal(null);
    api
      .getPortal(project.id)
      .then((p) => setPortal(p))
      .catch(() => setPortal(null))
      .finally(() => setLoading(false));
  }, [open, project?.id]);

  const portalUrl = portal && portal.slug
    ? safeHttpUrl(`${BASE}/portal/${portal.slug}`)
    : null;

  async function handlePublish() {
    if (!project?.id) return;
    setPublishing(true);
    try {
      const p = await api.publishPortal(project.id, {
        project_name: project.name ?? "project",
        agent_name: "nexus",
      });
      setPortal(p);
      toast("success", "Portal published", "A shareable link is now live.");
    } catch (err) {
      toast(
        "error",
        "Could not publish portal",
        err instanceof Error ? err.message : String(err),
      );
    } finally {
      setPublishing(false);
    }
  }

  async function handleCopy() {
    if (!portalUrl) return;
    try {
      await safeCopyToClipboard(portalUrl);
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch {
      toast("error", "Clipboard unavailable", "Copy the URL manually.");
    }
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <Globe className="w-4 h-4 text-glow-cyan" />
            Share your project
          </DialogTitle>
          <DialogDescription>
            Publish a read-only public portal that anyone can open in a browser.
          </DialogDescription>
        </DialogHeader>

        {loading ? (
          <div className="py-8 flex items-center justify-center text-slate-400 text-sm">
            <Loader2 className="w-4 h-4 animate-spin mr-2" />
            Loading portal status…
          </div>
        ) : portalUrl ? (
          <div className="space-y-3">
            <div className="rounded-lg border border-white/10 bg-white/[0.03] p-3 flex items-center gap-2">
              <code className="flex-1 text-xs font-mono text-slate-200 truncate">
                {portalUrl}
              </code>
              <Button
                size="sm"
                variant="outline"
                onClick={handleCopy}
                aria-label="Copy share link"
                className="flex-shrink-0"
              >
                {copied ? (
                  <>
                    <Check className="w-3.5 h-3.5 mr-1.5" /> Copied
                  </>
                ) : (
                  <>
                    <Copy className="w-3.5 h-3.5 mr-1.5" /> Copy
                  </>
                )}
              </Button>
              <Button
                size="sm"
                variant="outline"
                asChild
                aria-label="Open portal in a new tab"
                className="flex-shrink-0"
              >
                <a href={portalUrl} target="_blank" rel="noopener noreferrer">
                  <ExternalLink className="w-3.5 h-3.5" />
                </a>
              </Button>
            </div>
            <Button
              variant="outline"
              size="sm"
              onClick={handlePublish}
              disabled={publishing}
              className="w-full"
            >
              {publishing ? (
                <>
                  <Loader2 className="w-3.5 h-3.5 mr-1.5 animate-spin" />
                  Re-publishing…
                </>
              ) : (
                <>
                  <Rocket className="w-3.5 h-3.5 mr-1.5" />
                  Re-publish latest build
                </>
              )}
            </Button>
          </div>
        ) : (
          <div className="space-y-3">
            <p className="text-sm text-slate-400">
              No portal published yet. Publish one to get a shareable URL.
            </p>
            <Button
              onClick={handlePublish}
              disabled={publishing}
              className="w-full bg-gradient-to-r from-glow-cyan to-glow-blue hover:brightness-110 text-white"
            >
              {publishing ? (
                <>
                  <Loader2 className="w-4 h-4 mr-2 animate-spin" />
                  Publishing…
                </>
              ) : (
                <>
                  <Rocket className="w-4 h-4 mr-2" />
                  Publish portal
                </>
              )}
            </Button>
          </div>
        )}
      </DialogContent>
    </Dialog>
  );
}
