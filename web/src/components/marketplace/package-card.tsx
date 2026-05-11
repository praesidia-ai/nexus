"use client";

import Link from "next/link";
import { Bot, Package, Workflow, Wrench, Download, Star } from "lucide-react";
import { cn } from "@/lib/utils";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import type { MarketplacePackage, PackageKind } from "@/lib/marketplace-api";

const KIND_ICON: Record<PackageKind, React.ReactNode> = {
  agent: <Bot className="h-5 w-5" />,
  plugin: <Package className="h-5 w-5" />,
  workflow: <Workflow className="h-5 w-5" />,
  tool: <Wrench className="h-5 w-5" />,
};

const KIND_COLOR: Record<PackageKind, string> = {
  agent: "bg-violet-500/15 text-violet-400",
  plugin: "bg-blue-500/15 text-blue-400",
  workflow: "bg-emerald-500/15 text-emerald-400",
  tool: "bg-amber-500/15 text-amber-400",
};

interface Props {
  pkg: MarketplacePackage;
  onInstall: (name: string) => void;
  onUninstall: (name: string) => void;
  installing: boolean;
}

export function PackageCard({ pkg, onInstall, onUninstall, installing }: Props) {
  return (
    <div className="flex flex-col gap-3 rounded-xl border border-white/5 bg-white/2 p-4 hover:border-white/10 hover:bg-white/4 transition-all">
      <div className="flex items-start justify-between gap-2">
        <div className="flex items-center gap-2 min-w-0">
          <span className={cn("flex h-9 w-9 shrink-0 items-center justify-center rounded-lg", KIND_COLOR[pkg.kind])}>
            {KIND_ICON[pkg.kind]}
          </span>
          <div className="min-w-0">
            <Link
              href={`/marketplace/${pkg.name}`}
              className="truncate text-sm font-medium hover:text-violet-400 transition-colors"
            >
              {pkg.name}
            </Link>
            <p className="text-xs text-white/40">{pkg.author}</p>
          </div>
        </div>
        <Badge variant="secondary" className="shrink-0 text-xs">
          {pkg.kind}
        </Badge>
      </div>

      <p className="text-xs text-white/60 line-clamp-2">{pkg.description}</p>

      <div className="flex flex-wrap gap-1">
        {pkg.keywords.slice(0, 3).map((kw) => (
          <span key={kw} className="rounded-full bg-white/5 px-2 py-0.5 text-[10px] text-white/40">
            {kw}
          </span>
        ))}
      </div>

      <div className="mt-auto flex items-center justify-between">
        <div className="flex items-center gap-3 text-xs text-white/40">
          <span className="flex items-center gap-1">
            <Download className="h-3 w-3" />
            {pkg.downloads.toLocaleString()}
          </span>
          {pkg.rating !== null && (
            <span className="flex items-center gap-1">
              <Star className="h-3 w-3 fill-amber-400 text-amber-400" />
              {pkg.rating.toFixed(1)}
            </span>
          )}
          <span>v{pkg.latestVersion}</span>
        </div>
        {pkg.installed ? (
          <Button
            size="sm"
            variant="ghost"
            className="h-7 text-xs text-red-400 hover:text-red-300"
            onClick={() => onUninstall(pkg.name)}
            disabled={installing}
          >
            Uninstall
          </Button>
        ) : (
          <Button
            size="sm"
            variant="ghost"
            className="h-7 text-xs text-violet-400 hover:text-violet-300"
            onClick={() => onInstall(pkg.name)}
            disabled={installing}
          >
            {installing ? "Installing…" : "Install"}
          </Button>
        )}
      </div>
    </div>
  );
}
