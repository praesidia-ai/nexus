"use client";

import { useState, useEffect, useCallback } from "react";
import { useParams } from "next/navigation";
import { useToast } from "@/components/toast";
import {
  Link2,
  Webhook,
  Plus,
  Trash2,
  CheckCircle2,
  XCircle,
  Loader2,
  Zap,
  Globe,
  Database,
  MessageSquare,
  Mail,
  GitBranch,
  Calendar,
  FileText,
  Shield,
  Settings2,
} from "lucide-react";
import { cn } from "@/lib/utils";
import { api, BASE } from "@/lib/api";
import { Card, CardHeader, CardTitle, CardContent } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";

interface Integration {
  id: string;
  name: string;
  description: string;
  icon: React.ElementType;
  color: string;
  category: string;
  status: "available" | "connected" | "coming_soon";
  connectUrl?: string;
}

const INTEGRATIONS: Integration[] = [
  {
    id: "github",
    name: "GitHub",
    description: "Sync repos, create PRs, manage issues",
    icon: GitBranch,
    color: "text-white",
    category: "Development",
    status: "available",
  },
  {
    id: "slack",
    name: "Slack",
    description: "Send notifications, receive commands",
    icon: MessageSquare,
    color: "text-purple-400",
    category: "Communication",
    status: "available",
  },
  {
    id: "discord",
    name: "Discord",
    description: "Bot integration for Discord servers",
    icon: MessageSquare,
    color: "text-indigo-400",
    category: "Communication",
    status: "coming_soon",
  },
  {
    id: "notion",
    name: "Notion",
    description: "Sync pages, create databases",
    icon: FileText,
    color: "text-white",
    category: "Productivity",
    status: "coming_soon",
  },
  {
    id: "linear",
    name: "Linear",
    description: "Track issues, manage projects",
    icon: Zap,
    color: "text-violet-400",
    category: "Project Management",
    status: "coming_soon",
  },
  {
    id: "jira",
    name: "Jira",
    description: "Sync tickets, update statuses",
    icon: Shield,
    color: "text-blue-400",
    category: "Project Management",
    status: "coming_soon",
  },
  {
    id: "postgres",
    name: "PostgreSQL",
    description: "Connect to external databases",
    icon: Database,
    color: "text-blue-400",
    category: "Data",
    status: "available",
  },
  {
    id: "email",
    name: "Email (SMTP)",
    description: "Send emails from agents",
    icon: Mail,
    color: "text-amber-400",
    category: "Communication",
    status: "available",
  },
  {
    id: "calendar",
    name: "Google Calendar",
    description: "Schedule events, check availability",
    icon: Calendar,
    color: "text-emerald-400",
    category: "Productivity",
    status: "coming_soon",
  },
  {
    id: "openai",
    name: "OpenAI",
    description: "GPT, Whisper, DALL-E, TTS APIs",
    icon: Zap,
    color: "text-emerald-400",
    category: "AI",
    status: "available",
  },
  {
    id: "anthropic",
    name: "Anthropic",
    description: "Claude API for advanced reasoning",
    icon: Zap,
    color: "text-amber-400",
    category: "AI",
    status: "available",
  },
  {
    id: "custom_webhook",
    name: "Custom Webhook",
    description: "Send/receive events via HTTP webhooks",
    icon: Webhook,
    color: "text-pink-400",
    category: "Custom",
    status: "available",
  },
];

interface WebhookEntry {
  id: string;
  url: string;
  events: string[];
  active: boolean;
  created_at?: string;
}

type Tab = "catalog" | "webhooks" | "mcp";

export default function IntegrationsPage() {
  useParams<{ projectId: string }>();
  const { toast } = useToast();
  const [tab, setTab] = useState<Tab>("catalog");
  const [filter, setFilter] = useState<string>("all");
  const [webhooks, setWebhooks] = useState<WebhookEntry[]>([]);
  const [webhookUrl, setWebhookUrl] = useState("");
  const [webhookEvents, setWebhookEvents] = useState("");
  const [webhookSecret, setWebhookSecret] = useState("");
  const [submittingWebhook, setSubmittingWebhook] = useState(false);
  const [loadingWebhooks, setLoadingWebhooks] = useState(false);
  const [busyWebhookId, setBusyWebhookId] = useState<string | null>(null);

  const [mcpServers, setMcpServers] = useState<{ name: string; url: string; status: string; tools: number }[]>([]);
  const [loadingMcp, setLoadingMcp] = useState(false);

  // Providers the user has actually configured (via Settings). We render live
  // "Connected" badges based on this rather than hardcoding them in the catalog.
  const [connectedProviders, setConnectedProviders] = useState<Set<string>>(new Set());

  useEffect(() => {
    let alive = true;
    api
      .listApiKeys()
      .then((res) => {
        if (!alive) return;
        const set = new Set<string>();
        for (const k of res?.providers ?? []) {
          if (k.configured) set.add(k.provider.toLowerCase());
        }
        setConnectedProviders(set);
      })
      .catch(() => {
        /* settings unreachable — leave badges off, don't lie */
      });
    return () => {
      alive = false;
    };
  }, []);

  const fetchWebhooks = useCallback(async () => {
    setLoadingWebhooks(true);
    try {
      const res = await fetch(`${BASE}/webhooks`);
      if (!res.ok) {
        throw new Error(`HTTP ${res.status}`);
      }
      const data = await res.json();
      setWebhooks(data.webhooks ?? data ?? []);
    } catch (err) {
      toast("error", "Could not load webhooks", err instanceof Error ? err.message : String(err));
    } finally {
      setLoadingWebhooks(false);
    }
  }, [toast]);

  const fetchMcp = useCallback(async () => {
    setLoadingMcp(true);
    try {
      const res = await fetch(`${BASE}/mcp/servers`);
      if (!res.ok) {
        throw new Error(`HTTP ${res.status}`);
      }
      const data = await res.json();
      setMcpServers(data.servers ?? data ?? []);
    } catch (err) {
      toast("error", "Could not load MCP servers", err instanceof Error ? err.message : String(err));
    } finally {
      setLoadingMcp(false);
    }
  }, [toast]);

  useEffect(() => {
    if (tab === "webhooks") fetchWebhooks();
    if (tab === "mcp") fetchMcp();
  }, [tab, fetchWebhooks, fetchMcp]);

  async function addWebhook() {
    if (!webhookUrl.trim()) return;
    setSubmittingWebhook(true);
    try {
      const events = webhookEvents
        .split(",")
        .map((e) => e.trim())
        .filter(Boolean);
      if (events.length === 0) {
        throw new Error("At least one event is required");
      }
      const secret =
        webhookSecret.trim() ||
        (typeof crypto !== "undefined" && "randomUUID" in crypto
          ? crypto.randomUUID().replace(/-/g, "")
          : Math.random().toString(36).slice(2) + Math.random().toString(36).slice(2));
      const res = await fetch(`${BASE}/webhooks`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          url: webhookUrl.trim(),
          events,
          secret,
        }),
      });
      if (!res.ok) {
        const text = await res.text().catch(() => "");
        throw new Error(text || `HTTP ${res.status}`);
      }
      toast("success", "Webhook registered", webhookUrl.trim());
      setWebhookUrl("");
      setWebhookEvents("");
      setWebhookSecret("");
      await fetchWebhooks();
    } catch (err) {
      toast("error", "Could not register webhook", err instanceof Error ? err.message : String(err));
    } finally {
      setSubmittingWebhook(false);
    }
  }

  async function deleteWebhook(id: string) {
    if (busyWebhookId) return;
    setBusyWebhookId(id);
    try {
      const res = await fetch(`${BASE}/webhooks/${id}`, { method: "DELETE" });
      if (!res.ok) {
        throw new Error(`HTTP ${res.status}`);
      }
      toast("success", "Webhook removed");
      await fetchWebhooks();
    } catch (err) {
      toast("error", "Failed to delete webhook", err instanceof Error ? err.message : String(err));
    } finally {
      setBusyWebhookId(null);
    }
  }

  async function testWebhook(id: string) {
    if (busyWebhookId) return;
    setBusyWebhookId(id);
    try {
      const res = await fetch(`${BASE}/webhooks/${id}/test`, { method: "POST" });
      if (!res.ok) {
        throw new Error(`HTTP ${res.status}`);
      }
      toast("success", "Test event sent");
    } catch (err) {
      toast("error", "Test webhook failed", err instanceof Error ? err.message : String(err));
    } finally {
      setBusyWebhookId(null);
    }
  }

  const categories = ["all", ...new Set(INTEGRATIONS.map(i => i.category))];
  const filtered = filter === "all" ? INTEGRATIONS : INTEGRATIONS.filter(i => i.category === filter);

  const TABS: { id: Tab; label: string; icon: React.ElementType }[] = [
    { id: "catalog", label: "Integrations", icon: Link2 },
    { id: "webhooks", label: "Webhooks", icon: Webhook },
    { id: "mcp", label: "MCP Servers", icon: Globe },
  ];

  return (
    <div className="flex-1 overflow-y-auto">
      <div className="max-w-6xl mx-auto p-6 space-y-6">
        {/* Header */}
        <div>
          <h1 className="text-2xl font-bold text-white flex items-center gap-3">
            <Link2 className="w-7 h-7 text-cyan-400" />
            Integrations
          </h1>
          <p className="text-sm text-slate-400 mt-1">
            Connect NEXUS to your tools, APIs, and services
          </p>
        </div>

        {/* Tabs */}
        <div className="flex gap-1 bg-white/[0.03] p-1 rounded-lg border border-white/[0.06] w-fit">
          {TABS.map(t => (
            <button
              key={t.id}
              onClick={() => setTab(t.id)}
              className={cn(
                "flex items-center gap-2 px-4 py-2 rounded-md text-sm transition-colors",
                tab === t.id
                  ? "bg-white/[0.08] text-white"
                  : "text-slate-400 hover:text-white hover:bg-white/[0.04]",
              )}
            >
              <t.icon className="w-4 h-4" />
              {t.label}
            </button>
          ))}
        </div>

        {/* Integration Catalog */}
        {tab === "catalog" && (
          <>
            <div className="flex gap-1.5 flex-wrap">
              {categories.map(cat => (
                <button
                  key={cat}
                  onClick={() => setFilter(cat)}
                  className={cn(
                    "rounded-full px-3 py-1 text-xs font-medium transition-colors capitalize",
                    filter === cat
                      ? "bg-cyan-600 text-white"
                      : "bg-white/5 text-white/50 hover:bg-white/10",
                  )}
                >
                  {cat}
                </button>
              ))}
            </div>

            <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
              {filtered.map(int => (
                <Card
                  key={int.id}
                  className={cn(
                    "bg-white/[0.03] border-white/[0.06] transition-colors hover:bg-white/[0.05]",
                    int.status === "coming_soon" && "opacity-60",
                  )}
                >
                  <CardContent className="p-4">
                    <div className="flex items-center gap-3 mb-2">
                      <div className="w-10 h-10 rounded-lg bg-white/[0.06] flex items-center justify-center">
                        <int.icon className={cn("w-5 h-5", int.color)} />
                      </div>
                      <div className="flex-1">
                        <p className="text-sm font-medium text-white">{int.name}</p>
                        <p className="text-[10px] text-slate-500">{int.category}</p>
                      </div>
                      {connectedProviders.has(int.id) && (
                        <Badge className="bg-emerald-500/20 text-emerald-300 text-[10px]">Connected</Badge>
                      )}
                      {int.status === "coming_soon" && (
                        <Badge className="bg-slate-500/20 text-slate-400 text-[10px]">Soon</Badge>
                      )}
                    </div>
                    <p className="text-xs text-slate-400">{int.description}</p>
                    {int.status === "available" && (
                      <Button
                        size="sm"
                        variant="outline"
                        className="mt-3 w-full border-white/10 hover:bg-white/5 text-xs"
                        onClick={() => {
                          if (int.id === "openai" || int.id === "anthropic") {
                            window.location.href = "/settings";
                          } else if (int.id === "custom_webhook") {
                            setTab("webhooks");
                          } else {
                            toast(
                              "info",
                              `${int.name} configuration`,
                              "Set credentials in Settings or via vault keys.",
                            );
                          }
                        }}
                      >
                        <Settings2 className="w-3 h-3 mr-1" /> Configure
                      </Button>
                    )}
                  </CardContent>
                </Card>
              ))}
            </div>
          </>
        )}

        {/* Webhooks */}
        {tab === "webhooks" && (
          <Card className="bg-white/[0.03] border-white/[0.06]">
            <CardHeader>
              <CardTitle className="text-base text-white flex items-center gap-2">
                <Webhook className="w-5 h-5 text-pink-400" />
                Webhooks
              </CardTitle>
            </CardHeader>
            <CardContent className="space-y-4">
              <div className="flex flex-col gap-2">
                <div className="flex gap-2">
                  <input
                    className="flex-1 bg-white/[0.05] border border-white/10 rounded-lg px-3 py-2 text-sm text-white placeholder-slate-500 focus:outline-none focus:ring-1 focus:ring-blue-500/50"
                    placeholder="https://your-service.com/webhook"
                    value={webhookUrl}
                    onChange={e => setWebhookUrl(e.target.value)}
                  />
                  <input
                    className="w-56 bg-white/[0.05] border border-white/10 rounded-lg px-3 py-2 text-sm text-white placeholder-slate-500 focus:outline-none focus:ring-1 focus:ring-blue-500/50"
                    placeholder="Events e.g. generation_completed"
                    value={webhookEvents}
                    onChange={e => setWebhookEvents(e.target.value)}
                  />
                </div>
                <div className="flex gap-2">
                  <input
                    className="flex-1 bg-white/[0.05] border border-white/10 rounded-lg px-3 py-2 text-sm text-white placeholder-slate-500 focus:outline-none focus:ring-1 focus:ring-blue-500/50"
                    placeholder="Shared secret (auto-generated if blank)"
                    value={webhookSecret}
                    onChange={e => setWebhookSecret(e.target.value)}
                  />
                  <Button
                    size="sm"
                    onClick={addWebhook}
                    disabled={!webhookUrl.trim() || !webhookEvents.trim() || submittingWebhook}
                  >
                    {submittingWebhook ? (
                      <Loader2 className="w-4 h-4 mr-1 animate-spin" />
                    ) : (
                      <Plus className="w-4 h-4 mr-1" />
                    )}
                    Add
                  </Button>
                </div>
              </div>

              {loadingWebhooks ? (
                <div className="text-center py-8"><Loader2 className="w-5 h-5 animate-spin mx-auto text-slate-400" /></div>
              ) : webhooks.length === 0 ? (
                <div className="text-center py-8 text-slate-500">
                  <Webhook className="w-8 h-8 mx-auto mb-2 opacity-30" />
                  <p className="text-sm">No webhooks configured</p>
                </div>
              ) : (
                <div className="space-y-2">
                  {webhooks.map(wh => (
                    <div key={wh.id} className="flex items-center justify-between p-3 rounded-lg bg-white/[0.02] border border-white/[0.04]">
                      <div className="flex items-center gap-3">
                        {wh.active !== false ? (
                          <CheckCircle2 className="w-4 h-4 text-emerald-400" />
                        ) : (
                          <XCircle className="w-4 h-4 text-red-400" />
                        )}
                        <div>
                          <p className="text-sm text-white font-mono">{wh.url}</p>
                          <div className="flex gap-1 mt-1">
                            {wh.events?.map(e => (
                              <Badge key={e} variant="secondary" className="text-[10px]">{e}</Badge>
                            ))}
                          </div>
                        </div>
                      </div>
                      <div className="flex items-center gap-1">
                        <button
                          type="button"
                          onClick={() => testWebhook(wh.id)}
                          disabled={busyWebhookId !== null}
                          aria-label="Send test event"
                          className="text-slate-500 hover:text-cyan-400 text-[11px] px-2 py-1 rounded hover:bg-white/5 disabled:opacity-40 disabled:cursor-not-allowed"
                          title="Send test event"
                        >
                          {busyWebhookId === wh.id ? (
                            <Loader2 className="w-3 h-3 animate-spin" />
                          ) : (
                            "Test"
                          )}
                        </button>
                        <button
                          type="button"
                          onClick={() => deleteWebhook(wh.id)}
                          disabled={busyWebhookId !== null}
                          aria-label="Delete webhook"
                          className="text-slate-500 hover:text-red-400 p-1 disabled:opacity-40 disabled:cursor-not-allowed"
                          title="Delete webhook"
                        >
                          {busyWebhookId === wh.id ? (
                            <Loader2 className="w-4 h-4 animate-spin" />
                          ) : (
                            <Trash2 className="w-4 h-4" />
                          )}
                        </button>
                      </div>
                    </div>
                  ))}
                </div>
              )}
            </CardContent>
          </Card>
        )}

        {/* MCP Servers */}
        {tab === "mcp" && (
          <Card className="bg-white/[0.03] border-white/[0.06]">
            <CardHeader>
              <CardTitle className="text-base text-white flex items-center gap-2">
                <Globe className="w-5 h-5 text-blue-400" />
                MCP Servers
              </CardTitle>
            </CardHeader>
            <CardContent>
              {loadingMcp ? (
                <div className="text-center py-8"><Loader2 className="w-5 h-5 animate-spin mx-auto text-slate-400" /></div>
              ) : mcpServers.length === 0 ? (
                <div className="text-center py-8 text-slate-500">
                  <Globe className="w-8 h-8 mx-auto mb-2 opacity-30" />
                  <p className="text-sm">No MCP servers connected</p>
                  <p className="text-xs mt-1">Connect an MCP server to expose tools to your agents</p>
                </div>
              ) : (
                <div className="space-y-2">
                  {mcpServers.map((srv, i) => (
                    <div key={i} className="flex items-center justify-between p-3 rounded-lg bg-white/[0.02] border border-white/[0.04]">
                      <div className="flex items-center gap-3">
                        <CheckCircle2 className="w-4 h-4 text-emerald-400" />
                        <div>
                          <p className="text-sm font-medium text-white">{srv.name}</p>
                          <p className="text-xs text-slate-500 font-mono">{srv.url}</p>
                        </div>
                      </div>
                      <div className="flex items-center gap-2">
                        <Badge className="bg-blue-500/20 text-blue-300 text-[10px]">{srv.tools} tools</Badge>
                        <Badge className={cn(
                          "text-[10px]",
                          srv.status === "connected" ? "bg-emerald-500/20 text-emerald-300" : "bg-red-500/20 text-red-300",
                        )}>
                          {srv.status}
                        </Badge>
                      </div>
                    </div>
                  ))}
                </div>
              )}
            </CardContent>
          </Card>
        )}
      </div>
    </div>
  );
}
