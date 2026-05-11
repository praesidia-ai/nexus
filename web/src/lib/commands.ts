import {
  Search,
  Sparkles,
  Settings,
  Home,
  Workflow,
  Zap,
  Users,
  Store,
  Bot,
  Play,
  Square,
  Shield,
  DollarSign,
  Heart,
  Brain,
  RefreshCw,
  Gauge,
  Server,
  Cpu,
  Building2,
  Network,
  Webhook,
  Lock,
  Plus,
  type LucideIcon,
} from "lucide-react";
import { allNavItems } from "@/lib/navigation";

export type CommandCategory =
  | "Recent"
  | "Actions"
  | "Navigation"
  | "Project"
  | "Agents"
  | "System";

export interface CommandAction {
  id: string;
  label: string;
  description?: string;
  icon: LucideIcon;
  category: CommandCategory;
  shortcut?: string;
  href?: string;
  requiresProject?: boolean;
  keywords?: string[];
}

export interface CommandContext {
  projectId: string | null;
}

const RECENT_KEY = "nexus-recent-commands";
const MAX_RECENT = 5;

export function getRecentCommandIds(): string[] {
  if (typeof window === "undefined") return [];
  try {
    const stored = localStorage.getItem(RECENT_KEY);
    return stored ? (JSON.parse(stored) as string[]) : [];
  } catch {
    return [];
  }
}

export function addRecentCommand(commandId: string): void {
  if (typeof window === "undefined") return;
  try {
    const recent = getRecentCommandIds().filter((id) => id !== commandId);
    recent.unshift(commandId);
    localStorage.setItem(RECENT_KEY, JSON.stringify(recent.slice(0, MAX_RECENT)));
  } catch {
    // ignore storage errors
  }
}

interface ScoredCommand {
  command: CommandAction;
  score: number;
}

export function fuzzySearch(
  commands: CommandAction[],
  query: string,
): CommandAction[] {
  if (!query.trim()) return commands;

  const lower = query.toLowerCase();
  const chars = lower.split("");

  const scored: ScoredCommand[] = [];

  for (const cmd of commands) {
    const targets = [
      cmd.label,
      cmd.description ?? "",
      cmd.category,
      ...(cmd.keywords ?? []),
    ];

    let bestScore = 0;

    for (const target of targets) {
      const targetLower = target.toLowerCase();

      if (targetLower.includes(lower)) {
        const bonus = targetLower.startsWith(lower) ? 100 : 50;
        bestScore = Math.max(bestScore, bonus + (100 - targetLower.length));
        continue;
      }

      let charIdx = 0;
      let consecutive = 0;
      let score = 0;

      for (
        let i = 0;
        i < targetLower.length && charIdx < chars.length;
        i++
      ) {
        if (targetLower[i] === chars[charIdx]) {
          charIdx++;
          consecutive++;
          score += consecutive * 2;
          if (
            i === 0 ||
            targetLower[i - 1] === " " ||
            targetLower[i - 1] === "-"
          ) {
            score += 5;
          }
        } else {
          consecutive = 0;
        }
      }

      if (charIdx === chars.length) {
        bestScore = Math.max(bestScore, score);
      }
    }

    if (bestScore > 0) {
      scored.push({ command: cmd, score: bestScore });
    }
  }

  scored.sort((a, b) => b.score - a.score);
  return scored.map((s) => s.command);
}

export function buildCommands(ctx: CommandContext): CommandAction[] {
  const { projectId } = ctx;
  const p = projectId;

  const navItems = allNavItems();

  const navCommands: CommandAction[] = navItems.map((item) => ({
    id: `nav-${item.id}`,
    label: item.label,
    description: `Go to ${item.label}`,
    icon: item.icon,
    category: "Navigation" as const,
    requiresProject: true,
    href: p ? item.href(p) : undefined,
    keywords: item.keywords,
  }));

  const commands: CommandAction[] = [
    // Actions
    {
      id: "new-generation",
      label: "New Chat",
      description: "Start a new conversation",
      icon: Sparkles,
      category: "Actions",
      shortcut: "\u2318\u21e7G",
      href: p ? `/${p}` : "/",
      keywords: ["create", "generate", "build", "chat"],
    },
    {
      id: "build-ai-company",
      label: "Build AI Company",
      description: "Chat with the LLM — design a full team of AI employees",
      icon: Sparkles,
      category: "Actions",
      requiresProject: true,
      href: p ? `/${p}/agents/create` : undefined,
      keywords: [
        "new", "create", "wizard", "design", "agents", "zeroclaw",
        "company", "team", "hire", "employees", "startup", "org",
      ],
    },
    {
      id: "compose-workflow",
      label: "Compose Workflow",
      description: "Open the visual DAG canvas and build a workflow",
      icon: Network,
      category: "Actions",
      requiresProject: true,
      href: p ? `/${p}/workflows/compose` : undefined,
      keywords: ["canvas", "dag", "builder", "compose", "visual", "workflow"],
    },
    {
      id: "quick-create-agent",
      label: "Quick Create Agent",
      description: "Pick a roster preset (Nova, Atlas, Kai …) and deploy",
      icon: Plus,
      category: "Actions",
      requiresProject: true,
      href: p ? `/${p}/agents?quickCreate=1` : undefined,
      keywords: ["preset", "roster", "nova", "atlas", "kai", "luna", "orion"],
    },
    {
      id: "start-team",
      label: "Start Team",
      description: "Launch a multi-agent team",
      icon: Users,
      category: "Actions",
      requiresProject: true,
      href: p ? `/${p}/teams` : undefined,
      keywords: ["team", "multi-agent", "collaborate"],
    },
    {
      id: "run-workflow",
      label: "Run Workflow",
      description: "Execute a workflow pipeline",
      icon: Workflow,
      category: "Actions",
      requiresProject: true,
      href: p ? `/${p}/workflows` : undefined,
      keywords: ["pipeline", "dag"],
    },
    {
      id: "run-taste-gate",
      label: "Score Quality",
      description: "Run the quality scorer",
      icon: Gauge,
      category: "Actions",
      requiresProject: true,
      href: p ? `/${p}/quality` : undefined,
      keywords: ["taste", "quality", "score", "ux"],
    },
    {
      id: "generate-code",
      label: "Generate Code",
      description: "Run code generation pipeline",
      icon: Zap,
      category: "Actions",
      requiresProject: true,
      keywords: ["codegen", "build"],
    },

    // Global navigation
    {
      id: "nav-home",
      label: "Home",
      description: "Go to the home page",
      icon: Home,
      category: "Navigation",
      shortcut: "\u2318\u21e7H",
      href: "/",
    },
    {
      id: "nav-settings",
      label: "Settings",
      description: "Application settings and API keys",
      icon: Settings,
      category: "Navigation",
      shortcut: "\u2318,",
      href: "/settings",
      keywords: ["preferences", "config", "api keys"],
    },

    // Sidebar nav items (from shared definition)
    ...navCommands,

    // Power tools (command palette only)
    {
      id: "nav-business",
      label: "Business Dashboard",
      description: "CEO view -- teams, events, costs",
      icon: Building2,
      category: "Navigation",
      requiresProject: true,
      href: p ? `/${p}/business` : undefined,
      keywords: ["ceo", "teams", "dashboard", "business"],
    },
    {
      id: "nav-memory",
      label: "Memory",
      description: "Browse agent episodic and semantic memory",
      icon: Brain,
      category: "Navigation",
      requiresProject: true,
      href: p ? `/${p}/memory` : undefined,
      keywords: ["recall", "remember", "knowledge", "episodic"],
    },
    {
      id: "nav-vault",
      label: "Vault",
      description: "Secrets and environment variables",
      icon: Lock,
      category: "Navigation",
      requiresProject: true,
      href: p ? `/${p}/vault` : undefined,
      keywords: ["secrets", "env", "keys", "credentials"],
    },
    {
      id: "nav-processes",
      label: "Processes",
      description: "Kernel process monitor",
      icon: Cpu,
      category: "Navigation",
      requiresProject: true,
      href: p ? `/${p}/processes` : undefined,
      keywords: ["kernel", "scheduler", "running", "pid"],
    },
    {
      id: "nav-marketplace",
      label: "Marketplace",
      description: "Browse plugins and extensions",
      icon: Store,
      category: "Navigation",
      href: "/marketplace",
      keywords: ["plugins", "extensions", "addons"],
    },
    {
      id: "nav-admin",
      label: "Admin Panel",
      description: "System health, traces, security, federation",
      icon: Shield,
      category: "Navigation",
      href: "/admin",
      keywords: ["admin", "system", "health", "security"],
    },
    {
      id: "nav-global-agents",
      label: "All Agents (Global)",
      description: "Cross-project agent overview",
      icon: Bot,
      category: "Navigation",
      href: "/agents",
      keywords: ["global", "all", "agents"],
    },

    // Agents
    {
      id: "agents-run",
      label: "Run Agent",
      description: "Start an agent",
      icon: Play,
      category: "Agents",
      requiresProject: true,
      href: p ? `/${p}/agents` : undefined,
      keywords: ["start", "launch"],
    },
    {
      id: "agents-stop",
      label: "Stop Agent",
      description: "Stop a running agent",
      icon: Square,
      category: "Agents",
      requiresProject: true,
      href: p ? `/${p}/agents` : undefined,
      keywords: ["kill", "halt"],
    },

    // System
    {
      id: "sys-health",
      label: "Health Check",
      description: "View runtime health report",
      icon: Heart,
      category: "System",
      requiresProject: true,
      href: p ? `/${p}/observability` : undefined,
      keywords: ["status", "uptime", "diagnostics"],
    },
    {
      id: "sys-costs",
      label: "Cost Dashboard",
      description: "LLM cost tracking and budget",
      icon: DollarSign,
      category: "System",
      href: "/admin",
      keywords: ["money", "budget", "spending", "llm"],
    },
    {
      id: "sys-rebuild-graph",
      label: "Rebuild Code Graph",
      description: "Regenerate the code dependency graph",
      icon: RefreshCw,
      category: "System",
      requiresProject: true,
      keywords: ["graph", "dependencies", "rebuild"],
    },
    {
      id: "sys-providers",
      label: "LLM Providers",
      description: "Configure AI model providers",
      icon: Server,
      category: "System",
      href: "/settings",
      keywords: ["openai", "anthropic", "ollama", "model"],
    },
    {
      id: "sys-search",
      label: "Search Everything",
      description: "Search commands, pages, and actions",
      icon: Search,
      category: "System",
      shortcut: "\u2318K",
      keywords: ["find", "search", "lookup"],
    },
    {
      id: "nav-admin-mcp",
      label: "MCP Servers",
      description: "Manage connected MCP tool servers",
      icon: Server,
      category: "System",
      href: "/admin",
      keywords: ["mcp", "tools", "servers", "connect"],
    },
    {
      id: "nav-admin-federation",
      label: "Federation",
      description: "Manage peer connections and trust",
      icon: Network,
      category: "System",
      href: "/admin",
      keywords: ["federation", "peers", "connect", "trust"],
    },
    {
      id: "nav-admin-webhooks",
      label: "Webhooks",
      description: "Manage webhook integrations",
      icon: Webhook,
      category: "System",
      href: "/admin",
      keywords: ["webhook", "notifications", "events"],
    },
  ];

  return p
    ? commands
    : commands.filter((cmd) => !cmd.requiresProject);
}

const CATEGORY_ORDER: CommandCategory[] = [
  "Recent",
  "Actions",
  "Navigation",
  "Project",
  "Agents",
  "System",
];

export function sortedCategories(categories: CommandCategory[]): CommandCategory[] {
  return categories.sort(
    (a, b) => CATEGORY_ORDER.indexOf(a) - CATEGORY_ORDER.indexOf(b),
  );
}
