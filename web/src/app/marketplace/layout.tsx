import type { Metadata } from "next";

export const metadata: Metadata = {
  title: {
    default: "Marketplace — Nexus by Praesidia",
    template: "%s — Nexus Marketplace",
  },
  description:
    "Discover and install agents, plugins, workflows, and integrations for Nexus. Build AI-native applications faster with reviewed, rated packages from the Praesidia community.",
  keywords: [
    "Nexus",
    "Praesidia",
    "AI agents",
    "agent marketplace",
    "LLM plugins",
    "AI workflows",
    "agent skills",
    "Nexus marketplace",
  ],
  alternates: {
    canonical: "/marketplace",
  },
  openGraph: {
    type: "website",
    url: "https://nexus.praesidia.ai/marketplace",
    siteName: "Nexus by Praesidia",
    title: "Nexus Marketplace — AI agents, plugins, workflows",
    description:
      "Install vetted AI agents, plugins, and workflows in one click. Extend Nexus with packages built by the Praesidia community.",
    images: [
      {
        url: "/og/marketplace.png",
        width: 1200,
        height: 630,
        alt: "Nexus Marketplace",
      },
    ],
  },
  twitter: {
    card: "summary_large_image",
    title: "Nexus Marketplace — AI agents, plugins, workflows",
    description:
      "Install vetted AI agents, plugins, and workflows in one click. Extend Nexus with packages built by the Praesidia community.",
    images: ["/og/marketplace.png"],
  },
  robots: {
    index: true,
    follow: true,
  },
};

export default function MarketplaceLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  return children;
}
