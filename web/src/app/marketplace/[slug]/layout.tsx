import type { Metadata } from "next";
import { BASE } from "@/lib/api";

interface PackageSummary {
  name: string;
  kind?: string;
  latestVersion?: string;
  description?: string;
  keywords?: string[];
  categories?: string[];
  author?: string;
}

async function fetchPackage(slug: string): Promise<PackageSummary | null> {
  // Server-component fetch — keep it best-effort. A marketplace lookup
  // failure should not block the page from rendering; we just fall back
  // to generic metadata. A 3 s timeout prevents the SSR path from hanging.
  try {
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), 3000);
    const res = await fetch(
      `${BASE}/marketplace/packages/${encodeURIComponent(slug)}`,
      { signal: controller.signal, cache: "no-store" },
    );
    clearTimeout(timer);
    if (!res.ok) return null;
    return (await res.json()) as PackageSummary;
  } catch {
    return null;
  }
}

function truncate(s: string, max: number): string {
  return s.length <= max ? s : `${s.slice(0, max - 1).trimEnd()}…`;
}

export async function generateMetadata({
  params,
}: {
  params: Promise<{ slug: string }>;
}): Promise<Metadata> {
  const { slug } = await params;
  const pkg = await fetchPackage(slug);

  if (!pkg) {
    return {
      title: `${slug} — Nexus Marketplace`,
      description:
        "Install AI agents, plugins, and workflows for Nexus from the Praesidia marketplace.",
      alternates: { canonical: `/marketplace/${slug}` },
      robots: { index: true, follow: true },
    };
  }

  const title = `${pkg.name}${pkg.latestVersion ? ` v${pkg.latestVersion}` : ""} — Nexus Marketplace`;
  const description = truncate(
    pkg.description ||
      `Install ${pkg.name} for Nexus. Discover agents, plugins, and workflows in the Praesidia marketplace.`,
    200,
  );
  const keywords = [
    "Nexus",
    "Praesidia",
    "marketplace",
    pkg.name,
    ...(pkg.kind ? [pkg.kind] : []),
    ...(pkg.keywords ?? []),
    ...(pkg.categories ?? []),
  ];
  const canonical = `/marketplace/${pkg.name}`;
  const ogUrl = `https://nexus.praesidia.ai${canonical}`;

  return {
    title,
    description,
    keywords,
    alternates: { canonical },
    robots: { index: true, follow: true },
    authors: pkg.author ? [{ name: pkg.author }] : undefined,
    openGraph: {
      type: "article",
      url: ogUrl,
      siteName: "Nexus by Praesidia",
      title,
      description,
      images: [
        {
          url: "/og/marketplace.png",
          width: 1200,
          height: 630,
          alt: `${pkg.name} on Nexus Marketplace`,
        },
      ],
    },
    twitter: {
      card: "summary_large_image",
      title,
      description,
      images: ["/og/marketplace.png"],
    },
  };
}

export default function MarketplacePackageLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  return children;
}
