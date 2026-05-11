"use client";

import { useState } from "react";
import { useParams, useRouter } from "next/navigation";
import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import {
  ArrowLeft,
  Download,
  Calendar,
  Tag,
  Shield,
  CheckCircle2,
  XCircle,
  MessageSquare,
} from "lucide-react";
import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Skeleton } from "@/components/ui/skeleton";
import { StarRating } from "@/components/marketplace/star-rating";
import { marketplaceApi } from "@/lib/marketplace-api";

function CapabilityRow({ label, enabled }: { label: string; enabled: boolean }) {
  return (
    <div className="flex items-center justify-between py-1.5">
      <span className="text-xs text-white/50">{label}</span>
      {enabled ? (
        <CheckCircle2 className="h-3.5 w-3.5 text-emerald-400" />
      ) : (
        <XCircle className="h-3.5 w-3.5 text-white/20" />
      )}
    </div>
  );
}

export default function PackageDetailPage() {
  const { slug } = useParams<{ slug: string }>();
  const router = useRouter();
  const qc = useQueryClient();
  const [reviewRating, setReviewRating] = useState(5);
  const [reviewComment, setReviewComment] = useState("");
  const [installing, setInstalling] = useState(false);

  const { data: pkg, isLoading } = useQuery({
    queryKey: ["marketplace-pkg", slug],
    queryFn: () => marketplaceApi.getPackage(slug),
  });

  const installMutation = useMutation({
    mutationFn: () => marketplaceApi.install(slug),
    onMutate: () => setInstalling(true),
    onSettled: () => {
      setInstalling(false);
      qc.invalidateQueries({ queryKey: ["marketplace-pkg", slug] });
    },
  });

  const uninstallMutation = useMutation({
    mutationFn: () => marketplaceApi.uninstall(slug),
    onMutate: () => setInstalling(true),
    onSettled: () => {
      setInstalling(false);
      qc.invalidateQueries({ queryKey: ["marketplace-pkg", slug] });
    },
  });

  const reviewMutation = useMutation({
    mutationFn: () => marketplaceApi.submitReview(slug, reviewRating, reviewComment),
    onSuccess: () => {
      setReviewComment("");
      setReviewRating(5);
      qc.invalidateQueries({ queryKey: ["marketplace-pkg", slug] });
    },
  });

  if (isLoading) {
    return (
      <div className="flex flex-col gap-4 p-6">
        <Skeleton className="h-8 w-48" />
        <Skeleton className="h-4 w-full" />
        <Skeleton className="h-64 w-full" />
      </div>
    );
  }

  if (!pkg) {
    return (
      <div className="flex flex-col items-center justify-center gap-3 p-12 text-white/30">
        <p>Package not found.</p>
        <Button variant="ghost" size="sm" onClick={() => router.back()}>
          Go back
        </Button>
      </div>
    );
  }

  return (
    <div className="mx-auto max-w-5xl p-6 flex flex-col gap-6">
      {/* Header */}
      <div className="flex items-center gap-3">
        <button
          onClick={() => router.back()}
          className="flex h-8 w-8 items-center justify-center rounded-lg bg-white/5 hover:bg-white/10 transition-colors"
        >
          <ArrowLeft className="h-4 w-4" />
        </button>
        <span className="text-xs text-white/30">Marketplace</span>
      </div>

      <div className="flex flex-col gap-4 rounded-xl border border-white/5 bg-white/2 p-6 sm:flex-row sm:items-start">
        <div className="flex-1 flex flex-col gap-2">
          <div className="flex items-center gap-2 flex-wrap">
            <h1 className="text-xl font-semibold">{pkg.name}</h1>
            <Badge variant="secondary">{pkg.kind}</Badge>
            {pkg.installed && (
              <Badge variant="secondary" className="text-emerald-400">
                Installed v{pkg.installedVersion}
              </Badge>
            )}
          </div>
          <p className="text-sm text-white/50">{pkg.description}</p>
          <div className="flex items-center gap-4 text-xs text-white/40 flex-wrap">
            <span className="flex items-center gap-1">
              <Download className="h-3 w-3" />
              {pkg.downloads.toLocaleString()} installs
            </span>
            {pkg.rating !== null && (
              <span className="flex items-center gap-1.5">
                <StarRating value={pkg.rating} readonly size="sm" />
                {pkg.rating.toFixed(1)} ({pkg.reviewCount})
              </span>
            )}
            <span className="flex items-center gap-1">
              <Calendar className="h-3 w-3" />
              Updated {new Date(pkg.updatedAt).toLocaleDateString()}
            </span>
          </div>
          <div className="flex flex-wrap gap-1 mt-1">
            {pkg.keywords.map((kw) => (
              <span key={kw} className="rounded-full bg-white/5 px-2 py-0.5 text-[10px] text-white/40">
                <Tag className="mr-0.5 inline h-2.5 w-2.5" />
                {kw}
              </span>
            ))}
          </div>
        </div>

        <div className="flex flex-col gap-2 sm:items-end">
          {pkg.installed ? (
            <Button
              variant="destructive"
              size="sm"
              disabled={installing}
              onClick={() => uninstallMutation.mutate()}
            >
              {installing ? "Uninstalling…" : "Uninstall"}
            </Button>
          ) : (
            <Button
              size="sm"
              disabled={installing}
              onClick={() => installMutation.mutate()}
              className="bg-violet-600 hover:bg-violet-500"
            >
              {installing ? "Installing…" : "Install"}
            </Button>
          )}
          <span className="text-xs text-white/30">v{pkg.latestVersion}</span>
        </div>
      </div>

      <div className="grid grid-cols-1 gap-6 lg:grid-cols-3">
        {/* Main: README */}
        <div className="lg:col-span-2 flex flex-col gap-4">
          {/* README */}
          <div className="rounded-xl border border-white/5 bg-white/2 p-6">
            <h2 className="mb-4 text-sm font-semibold">README</h2>
            {/*
              README is rendered as plain text to avoid stored-XSS from
              untrusted marketplace packages. Switch to a sanitizing markdown
              renderer (e.g. DOMPurify + marked) before enabling HTML.
            */}
            <pre className="whitespace-pre-wrap break-words text-sm text-white/70 font-sans">
              {pkg.readme}
            </pre>
          </div>

          {/* Reviews */}
          <div className="rounded-xl border border-white/5 bg-white/2 p-6 flex flex-col gap-4">
            <h2 className="text-sm font-semibold flex items-center gap-2">
              <MessageSquare className="h-4 w-4 text-white/40" />
              Reviews ({pkg.reviewCount})
            </h2>

            {pkg.reviews.map((r) => (
              <div key={r.id} className="flex flex-col gap-1 border-t border-white/5 pt-3">
                <div className="flex items-center justify-between">
                  <span className="text-xs font-medium">{r.author}</span>
                  <StarRating value={r.rating} readonly size="sm" />
                </div>
                <p className="text-xs text-white/50">{r.comment}</p>
                <span className="text-[10px] text-white/20">
                  {new Date(r.createdAt).toLocaleDateString()}
                </span>
              </div>
            ))}

            {/* Submit review */}
            <div className="flex flex-col gap-2 border-t border-white/5 pt-4">
              <h3 className="text-xs font-medium text-white/60">Leave a review</h3>
              <StarRating value={reviewRating} onChange={setReviewRating} />
              <textarea
                value={reviewComment}
                onChange={(e) => setReviewComment(e.target.value)}
                rows={3}
                placeholder="Share your experience…"
                className="w-full resize-none rounded-lg border border-white/10 bg-white/5 p-2 text-xs text-white/70 placeholder-white/20 focus:outline-none focus:ring-1 focus:ring-violet-500"
              />
              <Button
                size="sm"
                variant="ghost"
                className="self-end text-violet-400"
                disabled={!reviewComment.trim() || reviewMutation.isPending}
                onClick={() => reviewMutation.mutate()}
              >
                {reviewMutation.isPending ? "Submitting…" : "Submit"}
              </Button>
            </div>
          </div>
        </div>

        {/* Sidebar */}
        <div className="flex flex-col gap-4">
          {/* Capabilities */}
          <div className="rounded-xl border border-white/5 bg-white/2 p-4">
            <h2 className="mb-2 flex items-center gap-2 text-xs font-semibold text-white/60">
              <Shield className="h-3.5 w-3.5" />
              Capabilities
            </h2>
            <CapabilityRow label="Network access" enabled={pkg.capabilities.network} />
            <CapabilityRow label="Filesystem access" enabled={pkg.capabilities.filesystem} />
            <CapabilityRow label="Shell/exec" enabled={pkg.capabilities.exec} />
            <CapabilityRow label="MCP tools" enabled={pkg.capabilities.mcp} />
            <CapabilityRow label="A2A delegation" enabled={pkg.capabilities.a2a} />
            <CapabilityRow label="Long-term memory" enabled={pkg.capabilities.memory} />
            <CapabilityRow label="Workflows" enabled={pkg.capabilities.workflows} />
          </div>

          {/* Resources */}
          <div className="rounded-xl border border-white/5 bg-white/2 p-4 text-xs">
            <h2 className="mb-2 font-semibold text-white/60">Resource Limits</h2>
            {pkg.resources.maxMemoryMb && (
              <div className="flex justify-between py-1 text-white/40">
                <span>Memory</span>
                <span>{pkg.resources.maxMemoryMb} MB</span>
              </div>
            )}
            {pkg.resources.timeoutSecs && (
              <div className="flex justify-between py-1 text-white/40">
                <span>Timeout</span>
                <span>{pkg.resources.timeoutSecs}s</span>
              </div>
            )}
            {pkg.resources.maxFuelM && (
              <div className="flex justify-between py-1 text-white/40">
                <span>Fuel</span>
                <span>{(pkg.resources.maxFuelM / 1e6).toFixed(0)}M</span>
              </div>
            )}
          </div>

          {/* Versions */}
          <div className="rounded-xl border border-white/5 bg-white/2 p-4">
            <h2 className="mb-2 text-xs font-semibold text-white/60">Versions</h2>
            <div className="flex flex-col gap-1">
              {pkg.versions.slice(0, 6).map((v) => (
                <div
                  key={v.version}
                  className={cn(
                    "flex items-center justify-between text-xs",
                    v.yanked ? "text-red-400/50 line-through" : "text-white/50",
                  )}
                >
                  <span>{v.version}</span>
                  <span className="text-white/20">{new Date(v.publishedAt).toLocaleDateString()}</span>
                </div>
              ))}
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
