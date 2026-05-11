"use client";

import { useState, useEffect, useCallback } from "react";
import { useRouter } from "next/navigation";
import {
  Database,
  ChevronRight,
  Loader2,
  Trash2,
  Brain,
  Eye,
  EyeOff,
  Plus,
  KeyRound,
} from "lucide-react";
import { api, type MaterializedTable, type KnowledgeItem, type VaultEntry } from "@/lib/api";
import { Card, CardHeader, CardContent } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { EmptyState } from "@/components/empty-state";
import { Tabs, TabsList, TabsTrigger, TabsContent } from "@/components/ui/tabs";

// ---------------------------------------------------------------------------
// Type helpers
// ---------------------------------------------------------------------------

const TYPE_VARIANT: Record<string, "info" | "purple" | "success" | "outline"> = {
  TEXT: "info", INTEGER: "purple", REAL: "success", BLOB: "outline",
};

const TYPE_ICONS: Record<string, string> = {
  entity: "👤", relationship: "🔗", rule: "📏", agent: "🤖",
  integration: "🔌", portal: "🌐", database: "🗄️", workflow: "⚙️",
};

const TYPE_BADGE_VARIANT: Record<string, "info" | "purple" | "warning" | "default" | "success" | "secondary"> = {
  entity: "info", agent: "purple", relationship: "warning", rule: "default",
  integration: "success", portal: "secondary", database: "secondary", workflow: "info",
};

// ---------------------------------------------------------------------------
// ProjectData component with sub-tabs
// ---------------------------------------------------------------------------

export function ProjectData({ projectId }: { projectId: string }) {
  return (
    <div className="flex flex-col h-full">
      <Tabs defaultValue="tables" className="flex-1 flex flex-col">
        <div className="px-6 pt-4">
          <TabsList className="bg-nexus-surface/60 border border-white/[0.06]">
            <TabsTrigger value="tables" className="data-[state=active]:bg-glow-cyan/[0.08]">
              <Database className="h-4 w-4 mr-2" />Tables
            </TabsTrigger>
            <TabsTrigger value="knowledge" className="data-[state=active]:bg-glow-cyan/[0.08]">
              <Brain className="h-4 w-4 mr-2" />Knowledge
            </TabsTrigger>
            <TabsTrigger value="vault" className="data-[state=active]:bg-glow-cyan/[0.08]">
              <KeyRound className="h-4 w-4 mr-2" />Vault
            </TabsTrigger>
          </TabsList>
        </div>

        <TabsContent value="tables" className="flex-1 mt-0 overflow-auto">
          <TablesPanel projectId={projectId} />
        </TabsContent>
        <TabsContent value="knowledge" className="flex-1 mt-0 overflow-auto">
          <KnowledgePanel projectId={projectId} />
        </TabsContent>
        <TabsContent value="vault" className="flex-1 mt-0 overflow-auto">
          <VaultPanel projectId={projectId} />
        </TabsContent>
      </Tabs>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Tables sub-panel
// ---------------------------------------------------------------------------

function TablesPanel({ projectId }: { projectId: string }) {
  const router = useRouter();
  const [tables, setTables] = useState<MaterializedTable[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    setLoading(true); setError(null);
    api.listTables(projectId)
      .then(setTables)
      .catch(() => setError("Failed to load tables"))
      .finally(() => setLoading(false));
  }, [projectId]);

  if (loading) return <div className="h-full flex items-center justify-center"><Loader2 className="w-6 h-6 animate-spin text-slate-400" /></div>;
  if (error) return <div className="p-8"><Card className="p-8 text-center"><p className="text-sm text-destructive">{error}</p></Card></div>;

  return (
    <div className="p-6">
      <p className="text-sm text-slate-400 mb-4">{tables.length} tables -- auto-generated from conversations</p>
      {tables.length === 0 ? (
        <EmptyState icon={Database} title="No tables yet" description="Ask Nexus to structure your data in the chat. It will create SQLite tables automatically." />
      ) : (
        <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
          {tables.map((t) => {
            const schema = t.schema_json as { fields?: Array<{ name: string; type: string }> };
            const fields = schema?.fields ?? [];
            return (
              <Card key={t.id} className="glass-card-hover cursor-pointer group" onClick={() => router.push(`/${projectId}/databases/${t.table_name}`)}>
                <CardHeader>
                  <div className="flex items-center gap-3">
                    <div className="w-9 h-9 rounded-lg bg-white/[0.06] flex items-center justify-center"><Database className="w-4 h-4 text-slate-400" /></div>
                    <div className="flex-1 min-w-0">
                      <p className="font-semibold text-sm text-slate-200">{t.table_name}</p>
                      <p className="text-xs text-slate-400">{fields.length} columns</p>
                    </div>
                    <ChevronRight className="w-4 h-4 text-slate-400/40 group-hover:text-glow-cyan transition-colors" />
                  </div>
                </CardHeader>
                <CardContent>
                  <div className="flex flex-wrap gap-1.5">
                    {fields.slice(0, 6).map((f) => (
                      <Badge key={f.name} variant="outline" className="gap-1.5 text-[11px]">
                        <span className="text-[9px] font-bold"><Badge variant={TYPE_VARIANT[f.type] ?? "outline"} className="px-1 py-0 text-[9px]">{f.type[0]}</Badge></span>
                        {f.name}
                      </Badge>
                    ))}
                    {fields.length > 6 && <span className="text-[11px] text-slate-400">+{fields.length - 6} more</span>}
                  </div>
                </CardContent>
              </Card>
            );
          })}
        </div>
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Knowledge sub-panel
// ---------------------------------------------------------------------------

function KnowledgePanel({ projectId }: { projectId: string }) {
  const [items, setItems] = useState<KnowledgeItem[]>([]);
  const [filter, setFilter] = useState<string>("all");
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    setLoading(true); setError(null);
    api.listKnowledge(projectId)
      .then(setItems)
      .catch(() => setError("Failed to load knowledge entries"))
      .finally(() => setLoading(false));
  }, [projectId]);

  const safeItems = Array.isArray(items) ? items : [];
  const types = [
    "all",
    ...Array.from(new Set(safeItems.map((i) => i.item_type ?? "other"))),
  ];
  const filtered =
    filter === "all" ? safeItems : safeItems.filter((i) => i.item_type === filter);

  async function remove(itemId: string) {
    try {
      await api.deleteKnowledge(projectId, itemId);
      setItems((prev) => prev.filter((i) => i.id !== itemId));
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to delete knowledge item");
    }
  }

  if (loading) return <div className="h-full flex items-center justify-center"><Loader2 className="w-6 h-6 animate-spin text-slate-400" /></div>;
  if (error) return <div className="p-8"><Card className="p-8 text-center"><p className="text-sm text-destructive">{error}</p></Card></div>;

  return (
    <div className="p-6">
      <p className="text-sm text-slate-400 mb-4">{items.length} entries -- built from your conversations</p>
      <div className="flex gap-2 mb-6 flex-wrap">
        {types.map((t) => (
          <Button key={t} variant={filter === t ? "secondary" : "ghost"} size="sm" onClick={() => setFilter(t)} className="capitalize">{t}</Button>
        ))}
      </div>
      {filtered.length === 0 ? (
        <EmptyState icon={Brain} title="No knowledge entries yet" description="Chat with Nexus to explore your business idea. As you talk, Nexus will build your knowledge base automatically." />
      ) : (
        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-3">
          {filtered.map((item) => (
            <Card key={item.id} className="glass-card-hover group flex items-start gap-4 p-4 transition-colors">
              <div className="w-9 h-9 rounded-lg bg-white/[0.05] border border-white/[0.08] flex items-center justify-center flex-shrink-0 text-lg">{item.icon ?? TYPE_ICONS[item.item_type] ?? "📌"}</div>
              <div className="flex-1 min-w-0">
                <div className="flex items-center gap-2 mb-1">
                  <span className="font-semibold text-sm">{item.name}</span>
                  <Badge variant={TYPE_BADGE_VARIANT[item.item_type] ?? "secondary"}>{item.item_type}</Badge>
                </div>
                {item.description && <p className="text-xs text-slate-400 leading-relaxed line-clamp-2">{item.description}</p>}
              </div>
              <Button variant="ghost" size="sm" onClick={() => remove(item.id)} className="opacity-0 group-hover:opacity-100 text-slate-400 hover:text-destructive hover:bg-destructive/10 transition-all h-7 w-7 p-0 flex-shrink-0">
                <Trash2 className="w-3.5 h-3.5" />
              </Button>
            </Card>
          ))}
        </div>
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Vault sub-panel
// ---------------------------------------------------------------------------

function VaultPanel({ projectId }: { projectId: string }) {
  const [entries, setEntries] = useState<VaultEntry[]>([]);
  const [revealed, setRevealed] = useState<Set<string>>(new Set());
  const [showAdd, setShowAdd] = useState(false);
  const [newKey, setNewKey] = useState("");
  const [newVal, setNewVal] = useState("");
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    setLoading(true); setError(null);
    api.listVault(projectId).then(setEntries).catch(() => setError("Failed to load vault entries")).finally(() => setLoading(false));
  }, [projectId]);

  useEffect(() => { load(); }, [load]);

  function toggleReveal(id: string) {
    setRevealed((prev) => { const next = new Set(prev); next.has(id) ? next.delete(id) : next.add(id); return next; });
  }

  async function add() {
    if (!newKey.trim() || !newVal.trim()) return;
    await api.setVault(projectId, newKey.trim(), newVal.trim());
    setNewKey(""); setNewVal(""); setShowAdd(false); await load();
  }

  async function remove(key: string) { await api.deleteVault(projectId, key); await load(); }

  if (loading) return <div className="h-full flex items-center justify-center"><Loader2 className="w-6 h-6 animate-spin text-slate-400" /></div>;
  if (error) return <div className="p-8"><Card className="p-8 text-center"><p className="text-sm text-destructive">{error}</p></Card></div>;

  return (
    <div className="p-6">
      <div className="flex items-center justify-between mb-4">
        <p className="text-sm text-slate-400">Securely stored project secrets</p>
        <Button onClick={() => setShowAdd(!showAdd)}><Plus className="w-3.5 h-3.5 mr-1.5" />Add Secret</Button>
      </div>

      {showAdd && (
        <Card className="mb-6 border-glow-cyan/20">
          <CardContent className="pt-5">
            <div className="grid grid-cols-2 gap-3 mb-3">
              <div><label className="text-xs font-medium text-slate-400 mb-1.5 block">Key</label><Input type="text" value={newKey} onChange={(e) => setNewKey(e.target.value)} placeholder="OPENAI_API_KEY" className="font-mono" /></div>
              <div><label className="text-xs font-medium text-slate-400 mb-1.5 block">Value</label><Input type="password" value={newVal} onChange={(e) => setNewVal(e.target.value)} placeholder="sk-..." className="font-mono" /></div>
            </div>
            <div className="flex gap-2"><Button onClick={add}>Save</Button><Button variant="ghost" onClick={() => setShowAdd(false)}>Cancel</Button></div>
          </CardContent>
        </Card>
      )}

      {entries.length === 0 ? (
        <EmptyState icon={KeyRound} title="No secrets stored" description="Store API keys, tokens, and credentials here. They are accessible by your agents." />
      ) : (
        <div className="space-y-2">
          {entries.map((entry) => (
            <Card key={entry.id} className="glass-card-hover group">
              <CardContent className="pt-5 flex items-center gap-4">
                <div className="w-8 h-8 rounded-lg bg-amber-500/10 flex items-center justify-center flex-shrink-0"><KeyRound className="w-4 h-4 text-amber-400" /></div>
                <div className="flex-1 min-w-0">
                  <p className="font-mono text-sm font-semibold text-slate-200">{entry.key}</p>
                  <p className="font-mono text-xs text-slate-400 mt-0.5">{revealed.has(entry.id) ? entry.value : "\u2022\u2022\u2022\u2022\u2022\u2022\u2022\u2022\u2022\u2022\u2022\u2022\u2022\u2022\u2022\u2022"}</p>
                </div>
                <div className="flex items-center gap-1.5">
                  <Button variant="ghost" size="icon" onClick={() => toggleReveal(entry.id)} className="h-8 w-8 text-slate-400">
                    {revealed.has(entry.id) ? <EyeOff className="w-3.5 h-3.5" /> : <Eye className="w-3.5 h-3.5" />}
                  </Button>
                  <Button variant="ghost" size="icon" onClick={() => remove(entry.key)} className="opacity-0 group-hover:opacity-100 h-8 w-8 text-destructive hover:text-destructive hover:bg-destructive/10 transition-all">
                    <Trash2 className="w-3.5 h-3.5" />
                  </Button>
                </div>
              </CardContent>
            </Card>
          ))}
        </div>
      )}
    </div>
  );
}
