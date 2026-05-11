import {
  MessageSquare,
  FileCode,
  Hammer,
  Database,
  Users,
  Bot,
  Globe,
  Workflow,
  ShieldCheck,
  Sparkles,
  Activity,
  Shield,
  Scale,
  Network,
  Rocket,
  Brain,
  Link2,
  type LucideIcon,
} from "lucide-react";

export interface NavItem {
  id: string;
  label: string;
  href: (projectId: string) => string;
  icon: LucideIcon;
  match: (pathname: string, projectId: string) => boolean;
  keywords?: string[];
}

export interface NavGroup {
  id: string;
  label: string;
  items: NavItem[];
}

function matchRoute(path: string, id: string, segment: string): boolean {
  const base = `/${id}/${segment}`;
  return path === base || path.startsWith(`${base}/`);
}

export const NAV_GROUPS: NavGroup[] = [
  {
    id: "workspace",
    label: "Workspace",
    items: [
      {
        id: "chat",
        label: "Chat",
        href: (id) => `/${id}`,
        icon: MessageSquare,
        match: (path, id) => path === `/${id}` || path === `/${id}/`,
        keywords: ["talk", "ask", "converse", "generate"],
      },
      {
        id: "files",
        label: "Files",
        href: (id) => `/${id}/files`,
        icon: FileCode,
        match: (path, id) => matchRoute(path, id, "files"),
        keywords: ["code", "editor", "source"],
      },
      {
        id: "build",
        label: "Build",
        href: (id) => `/${id}/build`,
        icon: Hammer,
        match: (path, id) => matchRoute(path, id, "build"),
        keywords: ["compile", "logs", "console"],
      },
      {
        id: "data",
        label: "Data",
        href: (id) => `/${id}/data`,
        icon: Database,
        match: (path, id) => matchRoute(path, id, "data"),
        keywords: ["tables", "sqlite", "knowledge", "vault"],
      },
    ],
  },
  {
    id: "collaborate",
    label: "Collaborate",
    items: [
      {
        id: "teams",
        label: "Teams",
        href: (id) => `/${id}/teams`,
        icon: Users,
        match: (path, id) => matchRoute(path, id, "teams"),
        keywords: ["team", "group", "multi-agent"],
      },
      {
        id: "agents",
        label: "Agents",
        href: (id) => `/${id}/agents`,
        icon: Bot,
        match: (path, id) => matchRoute(path, id, "agents"),
        keywords: ["bots", "ai"],
      },
      {
        id: "apps",
        label: "Apps",
        href: (id) => `/${id}/portal`,
        icon: Globe,
        match: (path, id) => matchRoute(path, id, "portal"),
        keywords: ["portal", "publish", "deploy", "client"],
      },
    ],
  },
  {
    id: "automate",
    label: "Automate",
    items: [
      {
        id: "workflows",
        label: "Workflows",
        href: (id) => `/${id}/workflows`,
        icon: Workflow,
        match: (path, id) => matchRoute(path, id, "workflows"),
        keywords: ["pipeline", "dag", "automation"],
      },
      {
        id: "approvals",
        label: "Approvals",
        href: (id) => `/${id}/approvals`,
        icon: ShieldCheck,
        match: (path, id) => matchRoute(path, id, "approvals"),
        keywords: ["hitl", "approve", "reject", "pending", "gates"],
      },
      {
        id: "quality",
        label: "Quality",
        href: (id) => `/${id}/quality`,
        icon: Sparkles,
        match: (path, id) => matchRoute(path, id, "quality"),
        keywords: ["taste", "score", "design", "ux"],
      },
    ],
  },
  {
    id: "observe",
    label: "Observe",
    items: [
      {
        id: "observability",
        label: "Observability",
        href: (id) => `/${id}/observability`,
        icon: Activity,
        match: (path, id) => matchRoute(path, id, "observability"),
        keywords: ["traces", "metrics", "monitoring", "performance", "costs"],
      },
      {
        id: "audit",
        label: "Audit",
        href: (id) => `/${id}/audit`,
        icon: Shield,
        match: (path, id) => matchRoute(path, id, "audit"),
        keywords: ["security", "log", "history", "trail"],
      },
      {
        id: "learning",
        label: "Learning",
        href: (id) => `/${id}/learning`,
        icon: Brain,
        match: (path, id) => matchRoute(path, id, "learning"),
        keywords: ["memory", "self-improvement", "skills", "patterns"],
      },
    ],
  },
  {
    id: "govern",
    label: "Govern",
    items: [
      {
        id: "governance",
        label: "Governance",
        href: (id) => `/${id}/governance`,
        icon: Scale,
        match: (path, id) => matchRoute(path, id, "governance"),
        keywords: ["policies", "compliance", "pii", "kill-switch"],
      },
      {
        id: "federation",
        label: "Federation",
        href: (id) => `/${id}/federation`,
        icon: Network,
        match: (path, id) => matchRoute(path, id, "federation"),
        keywords: ["a2a", "peers", "delegate", "agent-card"],
      },
    ],
  },
  {
    id: "ship",
    label: "Ship",
    items: [
      {
        id: "deploy",
        label: "Deploy",
        href: (id) => `/${id}/deploy`,
        icon: Rocket,
        match: (path, id) => matchRoute(path, id, "deploy"),
        keywords: ["docker", "kubernetes", "vercel", "railway", "ssh"],
      },
      {
        id: "integrations",
        label: "Integrations",
        href: (id) => `/${id}/integrations`,
        icon: Link2,
        match: (path, id) => matchRoute(path, id, "integrations"),
        keywords: ["webhooks", "mcp", "github", "slack", "api"],
      },
    ],
  },
];

export function allNavItems(): NavItem[] {
  return NAV_GROUPS.flatMap((g) => g.items);
}

export function findActiveItem(
  pathname: string,
  projectId: string,
): NavItem | undefined {
  return allNavItems().find((item) => item.match(pathname, projectId));
}
