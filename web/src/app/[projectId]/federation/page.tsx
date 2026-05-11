"use client";

import { useState, useEffect, useCallback } from "react";
import { useParams } from "next/navigation";
import {
  Network,
  Globe,
  Unplug,
  Plus,
  Trash2,
  Activity,
  CheckCircle2,
  XCircle,
  Loader2,
  Send,
  Radio,
  Server,
  ArrowRightLeft,
  Copy,
  ExternalLink,
} from "lucide-react";
import { cn, safeHttpUrl, safeCopyToClipboard } from "@/lib/utils";;
import {
  api,
  type A2AAgentCard,
  type A2APeer,
  type FederationPeer,
} from "@/lib/api";
import { Card, CardHeader, CardTitle, CardContent } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { useToast } from "@/components/toast";

type Tab = "card" | "peers" | "federation" | "delegate";

export default function FederationPage() {
  const { projectId: _projectId } = useParams<{ projectId: string }>();
  const { toast } = useToast();
  const [tab, setTab] = useState<Tab>("card");
  const [loading, setLoading] = useState(true);

  const [agentCard, setAgentCard] = useState<A2AAgentCard | null>(null);
  const [peers, setPeers] = useState<A2APeer[]>([]);
  const [fedPeers, setFedPeers] = useState<FederationPeer[]>([]);
  const [healthStats, setHealthStats] = useState<{ total: number; healthy: number; unhealthy: number } | null>(null);

  const [discoverUrl, setDiscoverUrl] = useState("");
  const [discovering, setDiscovering] = useState(false);
  const [federateUrl, setFederateUrl] = useState("");
  const [federating, setFederating] = useState(false);

  const [delegateMessage, setDelegateMessage] = useState("");
  const [delegateSkill, setDelegateSkill] = useState("");
  const [delegating, setDelegating] = useState(false);
  const [delegateResult, setDelegateResult] = useState<Record<string, unknown> | null>(null);

  const [healthChecking, setHealthChecking] = useState(false);

  const fetchAll = useCallback(async () => {
    try {
      const [card, peerData, fedData] = await Promise.allSettled([
        api.getAgentCard(),
        api.listA2APeers(),
        api.listFederationPeers(),
      ]);
      if (card.status === "fulfilled") setAgentCard(card.value);
      if (peerData.status === "fulfilled") setPeers(peerData.value.peers ?? []);
      if (fedData.status === "fulfilled") setFedPeers(fedData.value.peers ?? []);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    fetchAll();
  }, [fetchAll]);

  async function handleDiscover() {
    if (!discoverUrl.trim()) return;
    setDiscovering(true);
    try {
      await api.discoverPeer(discoverUrl);
      setDiscoverUrl("");
      await fetchAll();
      toast("success", "Peer discovered");
    } catch (err) {
      toast("error", "Discover failed", err instanceof Error ? err.message : "Request failed");
    } finally {
      setDiscovering(false);
    }
  }

  async function handleRemovePeer(url: string) {
    try {
      await api.removePeer(url);
      await fetchAll();
      toast("success", "Peer removed");
    } catch (err) {
      toast("error", "Remove failed", err instanceof Error ? err.message : "Request failed");
    }
  }

  async function handleFederate() {
    if (!federateUrl.trim()) return;
    setFederating(true);
    try {
      await api.registerFederationPeer(federateUrl);
      setFederateUrl("");
      await fetchAll();
      toast("success", "Federation peer registered");
    } catch (err) {
      toast("error", "Registration failed", err instanceof Error ? err.message : "Request failed");
    } finally {
      setFederating(false);
    }
  }

  async function handleRemoveFedPeer(id: string) {
    try {
      await api.removeFederationPeer(id);
      await fetchAll();
      toast("success", "Federation peer removed");
    } catch (err) {
      toast("error", "Remove failed", err instanceof Error ? err.message : "Request failed");
    }
  }

  async function handleHealthCheck() {
    setHealthChecking(true);
    try {
      const stats = await api.federationHealthCheck();
      setHealthStats(stats);
      await fetchAll();
    } catch (err) {
      toast("error", "Health check failed", err instanceof Error ? err.message : "Request failed");
    } finally {
      setHealthChecking(false);
    }
  }

  async function handleDelegate() {
    if (!delegateMessage.trim()) return;
    setDelegating(true);
    setDelegateResult(null);
    try {
      const result = await api.delegateToFederation(delegateMessage, delegateSkill || undefined);
      setDelegateResult(result);
    } catch (err) {
      toast("error", "Delegation failed", err instanceof Error ? err.message : "Request failed");
    } finally {
      setDelegating(false);
    }
  }

  const TABS: { id: Tab; label: string; icon: React.ElementType }[] = [
    { id: "card", label: "Agent Card", icon: Server },
    { id: "peers", label: "A2A Peers", icon: Globe },
    { id: "federation", label: "Federation", icon: Network },
    { id: "delegate", label: "Delegate Task", icon: Send },
  ];

  if (loading) {
    return (
      <div className="flex-1 flex items-center justify-center">
        <Loader2 className="w-6 h-6 animate-spin text-slate-400" />
      </div>
    );
  }

  return (
    <div className="flex-1 overflow-y-auto">
      <div className="max-w-6xl mx-auto p-6 space-y-6">
        {/* Header */}
        <div className="flex items-center justify-between">
          <div>
            <h1 className="text-2xl font-bold text-white flex items-center gap-3">
              <Network className="w-7 h-7 text-indigo-400" />
              A2A Federation
            </h1>
            <p className="text-sm text-slate-400 mt-1">
              Agent-to-Agent protocol — discover, connect, and delegate across organizations
            </p>
          </div>
          <Button
            variant="outline"
            size="sm"
            onClick={handleHealthCheck}
            disabled={healthChecking}
            className="border-white/10 hover:bg-white/5"
          >
            {healthChecking ? <Loader2 className="w-4 h-4 mr-1 animate-spin" /> : <Activity className="w-4 h-4 mr-1" />}
            Health Check
          </Button>
        </div>

        {/* Stats */}
        <div className="grid grid-cols-1 md:grid-cols-4 gap-4">
          <Card className="bg-white/[0.03] border-white/[0.06]">
            <CardContent className="p-4 flex items-center gap-3">
              <div className="w-10 h-10 rounded-lg bg-indigo-500/20 flex items-center justify-center">
                <Server className="w-5 h-5 text-indigo-400" />
              </div>
              <div>
                <p className="text-xs text-slate-500">Protocol</p>
                <p className="text-lg font-semibold text-white">{agentCard?.protocol_version ?? "N/A"}</p>
              </div>
            </CardContent>
          </Card>
          <Card className="bg-white/[0.03] border-white/[0.06]">
            <CardContent className="p-4 flex items-center gap-3">
              <div className="w-10 h-10 rounded-lg bg-blue-500/20 flex items-center justify-center">
                <Globe className="w-5 h-5 text-blue-400" />
              </div>
              <div>
                <p className="text-xs text-slate-500">A2A Peers</p>
                <p className="text-lg font-semibold text-white">{peers.length}</p>
              </div>
            </CardContent>
          </Card>
          <Card className="bg-white/[0.03] border-white/[0.06]">
            <CardContent className="p-4 flex items-center gap-3">
              <div className="w-10 h-10 rounded-lg bg-purple-500/20 flex items-center justify-center">
                <Network className="w-5 h-5 text-purple-400" />
              </div>
              <div>
                <p className="text-xs text-slate-500">Federated Nodes</p>
                <p className="text-lg font-semibold text-white">{fedPeers.length}</p>
              </div>
            </CardContent>
          </Card>
          <Card className="bg-white/[0.03] border-white/[0.06]">
            <CardContent className="p-4 flex items-center gap-3">
              <div className={cn(
                "w-10 h-10 rounded-lg flex items-center justify-center",
                healthStats ? (healthStats.unhealthy === 0 ? "bg-emerald-500/20" : "bg-amber-500/20") : "bg-slate-500/20",
              )}>
                <Radio className={cn(
                  "w-5 h-5",
                  healthStats ? (healthStats.unhealthy === 0 ? "text-emerald-400" : "text-amber-400") : "text-slate-400",
                )} />
              </div>
              <div>
                <p className="text-xs text-slate-500">Health</p>
                <p className="text-lg font-semibold text-white">
                  {healthStats ? `${healthStats.healthy}/${healthStats.total}` : "Not checked"}
                </p>
              </div>
            </CardContent>
          </Card>
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

        {/* Tab content */}
        {tab === "card" && agentCard && (
          <Card className="bg-white/[0.03] border-white/[0.06]">
            <CardHeader>
              <CardTitle className="text-base text-white flex items-center gap-2">
                <Server className="w-5 h-5 text-indigo-400" />
                Your Agent Card
                <Badge className="ml-2 bg-indigo-500/20 text-indigo-300 text-[10px]">
                  v{agentCard.version}
                </Badge>
              </CardTitle>
            </CardHeader>
            <CardContent className="space-y-4">
              <div className="bg-white/[0.03] rounded-lg p-4 space-y-3">
                <div className="flex items-center justify-between">
                  <div>
                    <p className="text-lg font-semibold text-white">{agentCard.name}</p>
                    <p className="text-sm text-slate-400">{agentCard.description}</p>
                  </div>
                  <Button
                    variant="ghost"
                    size="sm"
                    onClick={() => void safeCopyToClipboard(agentCard.url)}
                    className="text-slate-400"
                  >
                    <Copy className="w-4 h-4 mr-1" /> Copy URL
                  </Button>
                </div>
                <div className="text-xs text-slate-500 font-mono break-all">{agentCard.url}</div>
                <div className="flex gap-2 flex-wrap">
                  {agentCard.capabilities.streaming && <Badge className="bg-emerald-500/20 text-emerald-300 text-[10px]">Streaming</Badge>}
                  {agentCard.capabilities.state_transition_history && <Badge className="bg-blue-500/20 text-blue-300 text-[10px]">State History</Badge>}
                  {agentCard.capabilities.push_notifications && <Badge className="bg-purple-500/20 text-purple-300 text-[10px]">Push Notifications</Badge>}
                </div>
              </div>

              <div>
                <p className="text-sm font-medium text-slate-300 mb-2">Published Skills</p>
                <div className="grid grid-cols-1 md:grid-cols-2 gap-2">
                  {agentCard.skills.map(s => (
                    <div key={s.id} className="bg-white/[0.03] rounded-lg p-3 border border-white/[0.04]">
                      <p className="text-sm font-medium text-white">{s.name}</p>
                      <p className="text-xs text-slate-400 mt-0.5">{s.description}</p>
                      <div className="flex gap-1 mt-2 flex-wrap">
                        {s.tags.map(t => (
                          <Badge key={t} variant="secondary" className="text-[10px]">{t}</Badge>
                        ))}
                      </div>
                    </div>
                  ))}
                </div>
              </div>
            </CardContent>
          </Card>
        )}

        {tab === "peers" && (
          <Card className="bg-white/[0.03] border-white/[0.06]">
            <CardHeader>
              <CardTitle className="text-base text-white flex items-center gap-2">
                <Globe className="w-5 h-5 text-blue-400" />
                A2A Peers
              </CardTitle>
            </CardHeader>
            <CardContent className="space-y-4">
              <div className="flex gap-2">
                <input
                  className="flex-1 bg-white/[0.05] border border-white/10 rounded-lg px-3 py-2 text-sm text-white placeholder-slate-500 focus:outline-none focus:ring-1 focus:ring-blue-500/50"
                  placeholder="Enter remote agent base URL, e.g. https://agent.example.com"
                  value={discoverUrl}
                  onChange={e => setDiscoverUrl(e.target.value)}
                  onKeyDown={e => e.key === "Enter" && handleDiscover()}
                />
                <Button size="sm" onClick={handleDiscover} disabled={discovering || !discoverUrl.trim()}>
                  {discovering ? <Loader2 className="w-4 h-4 mr-1 animate-spin" /> : <Plus className="w-4 h-4 mr-1" />}
                  Discover
                </Button>
              </div>

              {peers.length === 0 ? (
                <div className="text-center py-12 text-slate-500">
                  <Unplug className="w-10 h-10 mx-auto mb-3 opacity-30" />
                  <p>No A2A peers discovered yet</p>
                  <p className="text-xs mt-1">Enter a remote agent URL above to connect</p>
                </div>
              ) : (
                <div className="space-y-2">
                  {peers.map((p, i) => (
                    <div key={i} className="flex items-center justify-between p-3 rounded-lg bg-white/[0.02] border border-white/[0.04]">
                      <div className="flex items-center gap-3">
                        {p.healthy !== false ? (
                          <CheckCircle2 className="w-5 h-5 text-emerald-400" />
                        ) : (
                          <XCircle className="w-5 h-5 text-red-400" />
                        )}
                        <div>
                          <p className="text-sm font-medium text-white">{p.name || p.url}</p>
                          {p.name && <p className="text-xs text-slate-500 font-mono">{p.url}</p>}
                          {p.description && <p className="text-xs text-slate-400">{p.description}</p>}
                        </div>
                      </div>
                      <div className="flex items-center gap-2">
                        {p.version && <Badge variant="secondary" className="text-[10px]">v{p.version}</Badge>}
                        <Badge className="bg-blue-500/20 text-blue-300 text-[10px]">
                          {p.skills?.length ?? 0} skills
                        </Badge>
                        {safeHttpUrl(p.url) && (
                          <a
                            href={safeHttpUrl(p.url) ?? "#"}
                            target="_blank"
                            rel="noopener noreferrer"
                            className="text-slate-400 hover:text-white p-1"
                          >
                            <ExternalLink className="w-4 h-4" />
                          </a>
                        )}
                        <button onClick={() => handleRemovePeer(p.url)} className="text-slate-500 hover:text-red-400 p-1">
                          <Trash2 className="w-4 h-4" />
                        </button>
                      </div>
                    </div>
                  ))}
                </div>
              )}
            </CardContent>
          </Card>
        )}

        {tab === "federation" && (
          <Card className="bg-white/[0.03] border-white/[0.06]">
            <CardHeader>
              <CardTitle className="text-base text-white flex items-center gap-2">
                <Network className="w-5 h-5 text-purple-400" />
                Federation Mesh
              </CardTitle>
            </CardHeader>
            <CardContent className="space-y-4">
              <div className="flex gap-2">
                <input
                  className="flex-1 bg-white/[0.05] border border-white/10 rounded-lg px-3 py-2 text-sm text-white placeholder-slate-500 focus:outline-none focus:ring-1 focus:ring-blue-500/50"
                  placeholder="Nexus peer URL, e.g. https://nexus-peer.example.com"
                  value={federateUrl}
                  onChange={e => setFederateUrl(e.target.value)}
                  onKeyDown={e => e.key === "Enter" && handleFederate()}
                />
                <Button size="sm" onClick={handleFederate} disabled={federating || !federateUrl.trim()}>
                  {federating ? <Loader2 className="w-4 h-4 mr-1 animate-spin" /> : <ArrowRightLeft className="w-4 h-4 mr-1" />}
                  Federate
                </Button>
              </div>

              {fedPeers.length === 0 ? (
                <div className="text-center py-12 text-slate-500">
                  <Network className="w-10 h-10 mx-auto mb-3 opacity-30" />
                  <p>No federated peers yet</p>
                  <p className="text-xs mt-1">Connect to another Nexus instance to form a federation mesh</p>
                </div>
              ) : (
                <div className="space-y-2">
                  {fedPeers.map((p) => (
                    <div key={p.id} className="flex items-center justify-between p-3 rounded-lg bg-white/[0.02] border border-white/[0.04]">
                      <div className="flex items-center gap-3">
                        {p.healthy ? (
                          <CheckCircle2 className="w-5 h-5 text-emerald-400" />
                        ) : (
                          <XCircle className="w-5 h-5 text-red-400" />
                        )}
                        <div>
                          <p className="text-sm font-medium text-white">{p.name || p.url}</p>
                          <p className="text-xs text-slate-500 font-mono">{p.url}</p>
                          {p.last_seen && <p className="text-xs text-slate-500">Last seen: {p.last_seen}</p>}
                        </div>
                      </div>
                      <div className="flex items-center gap-2">
                        <Badge className={cn(
                          "text-[10px]",
                          p.healthy ? "bg-emerald-500/20 text-emerald-300" : "bg-red-500/20 text-red-300",
                        )}>
                          {p.healthy ? "Healthy" : "Unreachable"}
                        </Badge>
                        <button onClick={() => handleRemoveFedPeer(p.id)} className="text-slate-500 hover:text-red-400 p-1">
                          <Trash2 className="w-4 h-4" />
                        </button>
                      </div>
                    </div>
                  ))}
                </div>
              )}
            </CardContent>
          </Card>
        )}

        {tab === "delegate" && (
          <Card className="bg-white/[0.03] border-white/[0.06]">
            <CardHeader>
              <CardTitle className="text-base text-white flex items-center gap-2">
                <Send className="w-5 h-5 text-amber-400" />
                Delegate to Federation
              </CardTitle>
            </CardHeader>
            <CardContent className="space-y-4">
              <p className="text-sm text-slate-400">
                Send a task to a remote Nexus instance in your federation. The system will find the best-matching peer based on skills.
              </p>
              <textarea
                className="w-full bg-white/[0.05] border border-white/10 rounded-lg px-3 py-2 text-sm text-white placeholder-slate-500 focus:outline-none focus:ring-1 focus:ring-blue-500/50 min-h-[100px] resize-none"
                placeholder="Describe the task to delegate..."
                value={delegateMessage}
                onChange={e => setDelegateMessage(e.target.value)}
                rows={4}
              />
              <input
                className="w-full bg-white/[0.05] border border-white/10 rounded-lg px-3 py-2 text-sm text-white placeholder-slate-500 focus:outline-none focus:ring-1 focus:ring-blue-500/50"
                placeholder="Required skill (optional), e.g. generate_app"
                value={delegateSkill}
                onChange={e => setDelegateSkill(e.target.value)}
              />
              <Button onClick={handleDelegate} disabled={delegating || !delegateMessage.trim()}>
                {delegating ? <Loader2 className="w-4 h-4 mr-2 animate-spin" /> : <Send className="w-4 h-4 mr-2" />}
                Delegate Task
              </Button>

              {delegateResult && (
                <div className="bg-white/[0.03] rounded-lg p-4 mt-3">
                  <p className="text-xs text-slate-400 mb-2">Response from federated peer:</p>
                  <pre className="text-sm text-slate-300 font-mono whitespace-pre-wrap overflow-auto max-h-[300px]">
                    {JSON.stringify(delegateResult, null, 2)}
                  </pre>
                </div>
              )}
            </CardContent>
          </Card>
        )}
      </div>
    </div>
  );
}
