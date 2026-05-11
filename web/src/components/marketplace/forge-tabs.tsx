"use client";

// Community-reputation surface: Trending · Top Rated · Newest.
// Reads straight off the public `/forge/*` endpoints, no project
// context required. Sits above the keyword-search UI on the
// marketplace landing page — browse-by-signal and search-by-keyword
// serve different moments.

import { useEffect, useState } from "react";
import { Sparkles, Star, TrendingUp } from "lucide-react";
import { cn } from "@/lib/utils";
import { forgeApi, type ForgeListing, type ForgeSurface } from "@/lib/forge-api";

const TABS: Array<{ id: ForgeSurface; label: string; icon: React.ElementType }> = [
  { id: "trending", label: "Trending", icon: TrendingUp },
  { id: "top_rated", label: "Top Rated", icon: Star },
  { id: "newest", label: "Newest", icon: Sparkles },
];

export function ForgeTabs() {
  const [active, setActive] = useState<ForgeSurface>("trending");
  const [items, setItems] = useState<ForgeListing[] | null>(null);
  const [err, setErr] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    setItems(null);
    setErr(null);
    const load = async () => {
      try {
        const rows =
          active === "trending"
            ? await forgeApi.trending({ limit: 12 })
            : active === "top_rated"
              ? await forgeApi.topRated({ limit: 12 })
              : await forgeApi.newest({ limit: 12 });
        if (!cancelled) setItems(rows);
      } catch (e) {
        if (!cancelled) setErr(e instanceof Error ? e.message : String(e));
      }
    };
    void load();
    return () => {
      cancelled = true;
    };
  }, [active]);

  return (
    <section className="rounded-xl border border-white/5 bg-white/[0.02] p-5">
      <div className="mb-4 flex items-center justify-between">
        <div>
          <h2 className="text-sm font-semibold text-white/80">
            Community reputation
          </h2>
          <p className="text-[11px] text-white/40">
            Free marketplace — signals come from real installs and
            reviews, not promotion.
          </p>
        </div>
        <div className="flex gap-1 rounded-full border border-white/5 p-1">
          {TABS.map((t) => {
            const Icon = t.icon;
            return (
              <button
                key={t.id}
                onClick={() => setActive(t.id)}
                className={cn(
                  "inline-flex items-center gap-1.5 rounded-full px-3 py-1 text-xs",
                  active === t.id
                    ? "bg-white/[0.08] text-white"
                    : "text-white/50 hover:text-white/80",
                )}
              >
                <Icon className="h-3.5 w-3.5" />
                {t.label}
              </button>
            );
          })}
        </div>
      </div>

      {err && (
        <div className="rounded-lg border border-red-500/30 bg-red-500/10 p-3 text-xs text-red-300">
          {err}
        </div>
      )}

      {items === null && !err && (
        <div className="grid grid-cols-1 gap-2 sm:grid-cols-2 lg:grid-cols-3">
          {Array.from({ length: 6 }).map((_, i) => (
            <div
              key={i}
              className="h-20 animate-pulse rounded-lg border border-white/5 bg-white/[0.03]"
            />
          ))}
        </div>
      )}

      {items && items.length === 0 && (
        <p className="text-xs text-white/40">
          No listings on this surface yet. Be the first to publish to
          Nexus Forge.
        </p>
      )}

      {items && items.length > 0 && (
        <ul className="grid grid-cols-1 gap-2 sm:grid-cols-2 lg:grid-cols-3">
          {items.map((i) => (
            <ForgeCard key={i.id} listing={i} surface={active} />
          ))}
        </ul>
      )}
    </section>
  );
}

function ForgeCard({
  listing,
  surface,
}: {
  listing: ForgeListing;
  surface: ForgeSurface;
}) {
  const stars =
    listing.review_count > 0 ? listing.avg_rating.toFixed(1) : "–";
  const subtitle =
    surface === "newest"
      ? new Date(String(listing.score)).toLocaleDateString()
      : surface === "top_rated"
        ? `Bayesian ${Number(listing.score).toFixed(2)}`
        : `${listing.install_count_7d} installs / 7d`;

  return (
    <li className="rounded-lg border border-white/5 bg-white/[0.03] p-3 transition hover:border-white/10">
      <div className="flex items-start gap-2">
        <div className="text-xl">{listing.icon ?? "🧩"}</div>
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2">
            <p className="truncate text-sm font-medium text-white/90">
              {listing.name}
            </p>
            {listing.is_official && (
              <span className="rounded-full bg-sky-500/10 px-1.5 py-0.5 text-[10px] font-medium text-sky-300">
                official
              </span>
            )}
          </div>
          <p className="line-clamp-2 text-xs text-white/50">
            {listing.description}
          </p>
          <div className="mt-2 flex items-center gap-3 text-[11px] text-white/40">
            <span>{listing.install_count} installs</span>
            <span>★ {stars}</span>
            <span>{subtitle}</span>
          </div>
        </div>
      </div>
    </li>
  );
}
