"use client";

import { useState, useEffect, useCallback } from "react";
import { useParams, useSearchParams } from "next/navigation";
import {
  Play,
  Square,
  Trash2,
  Bot,
  Wrench,
  Cpu,
  Zap,
  Plus,
  Sparkles,
  Shield,
  Code2,
  BookOpen,
  Search,
  BarChart3,
  Cloud,
  Megaphone,
  Server,
  ClipboardList,
  HeadphonesIcon,
  Eye,
  ChevronDown,
  ChevronUp,
  Check,
  X,
  Copy,
  Loader2,
  Network,
} from "lucide-react";
import { useRouter } from "next/navigation";
import { cn, safeCopyToClipboard } from "@/lib/utils";;
import { api, type AgentDefinition, type AgentSkill } from "@/lib/api";
import { ModelSelector } from "@/components/model-selector";
import { PageHeader } from "@/components/page-header";
import { Card, CardContent } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { EmptyState } from "@/components/empty-state";
import { Separator } from "@/components/ui/separator";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogDescription } from "@/components/ui/dialog";
import { ScrollArea } from "@/components/ui/scroll-area";
import { QuickCreateAgentDialog } from "@/components/agent/quick-create-agent-dialog";
import { useToast } from "@/components/toast";

/* ------------------------------------------------------------------ */
/*  Constants                                                          */
/* ------------------------------------------------------------------ */

const STATUS_BADGE_VARIANT: Record<string, "success" | "secondary" | "destructive"> = {
  defined: "secondary",
  running: "success",
  stopped: "secondary",
  error: "destructive",
};

const STATUS_DOT: Record<string, string> = {
  defined: "bg-slate-600",
  running: "bg-emerald-400 animate-pulse",
  stopped: "bg-slate-600",
  error: "bg-red-400",
};

const CATEGORY_CONFIG: Record<string, { icon: typeof Code2; color: string; bg: string }> = {
  coding: { icon: Code2, color: "text-blue-400", bg: "bg-blue-500/10" },
  security: { icon: Shield, color: "text-red-400", bg: "bg-red-500/10" },
  research: { icon: Search, color: "text-purple-400", bg: "bg-purple-500/10" },
  writing: { icon: BookOpen, color: "text-emerald-400", bg: "bg-emerald-500/10" },
  data: { icon: BarChart3, color: "text-amber-400", bg: "bg-amber-500/10" },
  devops: { icon: Server, color: "text-orange-400", bg: "bg-orange-500/10" },
  cloud: { icon: Cloud, color: "text-cyan-400", bg: "bg-cyan-500/10" },
  marketing: { icon: Megaphone, color: "text-pink-400", bg: "bg-pink-500/10" },
  management: { icon: ClipboardList, color: "text-indigo-400", bg: "bg-indigo-500/10" },
  support: { icon: HeadphonesIcon, color: "text-teal-400", bg: "bg-teal-500/10" },
  general: { icon: Zap, color: "text-yellow-400", bg: "bg-yellow-500/10" },
  custom: { icon: Sparkles, color: "text-violet-400", bg: "bg-violet-500/10" },
};

function getCategoryConfig(category: string) {
  return CATEGORY_CONFIG[category] ?? CATEGORY_CONFIG.general;
}

/* ------------------------------------------------------------------ */
/*  SkillCard component                                                */
/* ------------------------------------------------------------------ */

function SkillCard({
  skill,
  assigned,
  onAssign,
  onUnassign,
  onPreview,
  loading,
}: {
  skill: AgentSkill;
  assigned: boolean;
  onAssign: () => void;
  onUnassign: () => void;
  onPreview: () => void;
  loading: boolean;
}) {
  const cat = getCategoryConfig(skill.category);

  return (
    <div
      className={cn(
        "rounded-xl border p-4 transition-all duration-300 group",
        assigned
          ? "border-primary/30 bg-glow-cyan/[0.03] shadow-[0_0_20px_rgba(124,58,237,0.06)]"
          : "border-white/[0.08] bg-white/[0.02] hover:bg-white/[0.04]"
      )}
    >
      <div className="flex items-start gap-3 mb-3">
        <div
          className={cn(
            "w-10 h-10 rounded-xl flex items-center justify-center border shrink-0",
            assigned ? "bg-glow-cyan/10 border-primary/20" : cat.bg + " border-white/[0.08]"
          )}
        >
          <span className="text-lg">{skill.icon}</span>
        </div>
        <div className="flex-1 min-w-0">
          <div className="flex items-center gap-2 mb-0.5">
            <span className="font-semibold text-sm">{skill.name}</span>
            {skill.is_builtin && (
              <Badge variant="outline" className="text-[9px] h-4 px-1.5">
                Built-in
              </Badge>
            )}
            {assigned && (
              <Badge variant="info" className="text-[9px] h-4 px-1.5 gap-0.5">
                <Check className="w-2.5 h-2.5" /> Active
              </Badge>
            )}
          </div>
          <p className="text-xs text-slate-400 line-clamp-2">
            {skill.description}
          </p>
        </div>
      </div>

      {/* Tools preview */}
      {skill.tools.length > 0 && (
        <div className="flex flex-wrap gap-1 mb-3">
          {skill.tools.slice(0, 5).map((t) => (
            <Badge key={t} variant="secondary" className="text-[10px] h-5">
              {t}
            </Badge>
          ))}
          {skill.tools.length > 5 && (
            <Badge variant="secondary" className="text-[10px] h-5">
              +{skill.tools.length - 5} more
            </Badge>
          )}
        </div>
      )}

      {/* Rules preview */}
      {skill.rules.length > 0 && (
        <div className="mb-3">
          <p className="text-[10px] text-slate-400 mb-1">
            {skill.rules.length} behavioral rules
          </p>
          <p className="text-xs text-slate-400/70 line-clamp-1 italic">
            &quot;{skill.rules[0]}&quot;
          </p>
        </div>
      )}

      {/* Actions */}
      <div className="flex items-center gap-2">
        {assigned ? (
          <Button
            size="sm"
            variant="outline"
            onClick={onUnassign}
            disabled={loading}
            className="h-7 text-xs gap-1.5 text-red-400 hover:text-red-400 hover:bg-red-500/10"
          >
            {loading ? <Loader2 className="w-3 h-3 animate-spin" /> : <X className="w-3 h-3" />}
            Remove
          </Button>
        ) : (
          <Button
            size="sm"
            variant="outline"
            onClick={onAssign}
            disabled={loading}
            className="h-7 text-xs gap-1.5 text-emerald-400 hover:text-emerald-400 hover:bg-emerald-500/10"
          >
            {loading ? <Loader2 className="w-3 h-3 animate-spin" /> : <Plus className="w-3 h-3" />}
            Add Skill
          </Button>
        )}
        <Button
          size="sm"
          variant="ghost"
          onClick={onPreview}
          className="h-7 text-xs gap-1.5"
        >
          <Eye className="w-3 h-3" />
          Preview
        </Button>
      </div>
    </div>
  );
}

/* ------------------------------------------------------------------ */
/*  PromptPreview dialog                                               */
/* ------------------------------------------------------------------ */

function PromptPreviewDialog({
  open,
  onClose,
  content,
  title,
}: {
  open: boolean;
  onClose: () => void;
  content: string;
  title: string;
}) {
  function handleCopy() {
    void safeCopyToClipboard(content);
  }

  return (
    <Dialog open={open} onOpenChange={onClose}>
      <DialogContent className="max-w-3xl max-h-[80vh]">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <Eye className="w-4 h-4" />
            {title}
          </DialogTitle>
          <DialogDescription>
            This is the composed system prompt that the agent will use.
          </DialogDescription>
        </DialogHeader>
        <div className="flex justify-end mb-2">
          <Button variant="outline" size="sm" className="gap-1.5 text-xs" onClick={handleCopy}>
            <Copy className="w-3 h-3" />
            Copy
          </Button>
        </div>
        <ScrollArea className="max-h-[60vh]">
          <pre className="bg-black/30 border border-white/[0.06] rounded-lg p-4 font-mono text-xs text-slate-400 whitespace-pre-wrap leading-relaxed">
            {content}
          </pre>
        </ScrollArea>
      </DialogContent>
    </Dialog>
  );
}

/* ------------------------------------------------------------------ */
/*  Main Page                                                          */
/* ------------------------------------------------------------------ */

export default function AgentsPage() {
  const params = useParams();
  const router = useRouter();
  const searchParams = useSearchParams();
  const { toast } = useToast();
  const projectId = params.projectId as string;

  const [agents, setAgents] = useState<AgentDefinition[]>([]);
  const [allSkills, setAllSkills] = useState<AgentSkill[]>([]);
  const [agentSkills, setAgentSkills] = useState<Record<string, AgentSkill[]>>({});
  const [expanded, setExpanded] = useState<string | null>(null);
  const [skillFilter, setSkillFilter] = useState<string>("all");
  const [skillLoading, setSkillLoading] = useState<Record<string, boolean>>({});
  const [promptPreview, setPromptPreview] = useState<{ open: boolean; content: string; title: string }>({
    open: false,
    content: "",
    title: "",
  });
  const [quickCreateOpen, setQuickCreateOpen] = useState(false);

  const loadAgents = useCallback(async () => {
    try {
      const agts = await api.listAgents(projectId);
      setAgents(agts);
    } catch { /* ignore */ }
  }, [projectId]);

  const loadSkills = useCallback(async () => {
    try {
      const skills = await api.listSkills(projectId);
      setAllSkills(skills);
    } catch {
      // Fallback to builtins
      try {
        const builtins = await api.listBuiltinSkills();
        setAllSkills(builtins);
      } catch { /* ignore */ }
    }
  }, [projectId]);

  const loadAgentSkills = useCallback(
    async (agentId: string) => {
      try {
        const skills = await api.listAgentSkills(projectId, agentId);
        setAgentSkills((prev) => ({ ...prev, [agentId]: skills }));
      } catch { /* ignore */ }
    },
    [projectId]
  );

  useEffect(() => {
    loadAgents();
    loadSkills();
  }, [loadAgents, loadSkills]);

  // Auto-open Quick Create when arriving via command palette (?quickCreate=1)
  useEffect(() => {
    if (searchParams?.get("quickCreate") === "1") {
      setQuickCreateOpen(true);
    }
  }, [searchParams]);

  // Load skills for expanded agent
  useEffect(() => {
    if (expanded) {
      loadAgentSkills(expanded);
    }
  }, [expanded, loadAgentSkills]);

  async function toggleRun(agent: AgentDefinition) {
    try {
      if (agent.status === "running") {
        await api.stopAgent(projectId, agent.id);
      } else {
        await api.runAgent(projectId, agent.id);
      }
      await loadAgents();
    } catch (err) {
      toast(
        "error",
        agent.status === "running" ? "Stop failed" : "Start failed",
        err instanceof Error ? err.message : "Request failed",
      );
    }
  }

  async function remove(agentId: string) {
    if (typeof window !== "undefined") {
      const name =
        agents.find((a) => a.id === agentId)?.name ?? "this agent";
      if (!window.confirm(`Delete ${name}? This cannot be undone.`)) return;
    }
    try {
      await api.deleteAgent(projectId, agentId);
      await loadAgents();
      toast("success", "Agent deleted");
    } catch (err) {
      toast("error", "Delete failed", err instanceof Error ? err.message : "Request failed");
    }
  }

  async function handleAssignSkill(agentId: string, skillId: string) {
    setSkillLoading((prev) => ({ ...prev, [`${agentId}-${skillId}`]: true }));
    try {
      await api.assignSkill(projectId, agentId, skillId);
      await loadAgentSkills(agentId);
    } catch { /* ignore */ }
    setSkillLoading((prev) => ({ ...prev, [`${agentId}-${skillId}`]: false }));
  }

  async function handleUnassignSkill(agentId: string, skillId: string) {
    setSkillLoading((prev) => ({ ...prev, [`${agentId}-${skillId}`]: true }));
    try {
      await api.unassignSkill(projectId, agentId, skillId);
      await loadAgentSkills(agentId);
    } catch { /* ignore */ }
    setSkillLoading((prev) => ({ ...prev, [`${agentId}-${skillId}`]: false }));
  }

  async function handlePreviewPrompt(agentId: string, agentName: string) {
    try {
      const result = await api.previewAgentPrompt(projectId, agentId);
      setPromptPreview({
        open: true,
        content: result.prompt,
        title: `${agentName} — Composed Prompt`,
      });
    } catch { /* ignore */ }
  }

  async function handlePreviewSkill(skill: AgentSkill) {
    let preview = skill.system_prompt;
    if (skill.rules.length > 0) {
      preview += "\n\n## Rules\n" + skill.rules.map((r, i) => `${i + 1}. ${r}`).join("\n");
    }
    if (skill.tools.length > 0) {
      preview += "\n\n## Tools\n" + skill.tools.map((t) => `- ${t}`).join("\n");
    }
    setPromptPreview({
      open: true,
      content: preview,
      title: `${skill.name} — Skill Prompt`,
    });
  }

  // Filter skills by category
  const filteredSkills =
    skillFilter === "all"
      ? allSkills
      : allSkills.filter((s) => s.category === skillFilter);

  const categories = ["all", ...new Set(allSkills.map((s) => s.category))];

  return (
    <div className="h-full overflow-y-auto scrollbar-thin p-8">
      <PageHeader
        title="Agents & Skills"
        subtitle={`${agents.length} agents, ${allSkills.length} skills available`}
        className="mb-6"
        actions={
          <div className="flex items-center gap-2">
            <Button
              variant="outline"
              onClick={() => setQuickCreateOpen(true)}
              className="gap-2"
            >
              <Plus className="w-4 h-4" />
              Quick Create
            </Button>
            <Button
              variant="outline"
              onClick={() => router.push(`/${projectId}/workflows/compose`)}
              className="gap-2"
            >
              <Network className="w-4 h-4" />
              Compose Workflow
            </Button>
            <Button
              onClick={() => router.push(`/${projectId}/agents/create`)}
              className="gap-2 bg-gradient-to-r from-glow-cyan to-glow-blue text-white shadow-glow-sm hover:shadow-glow-md hover:brightness-110"
            >
              <Sparkles className="w-4 h-4" />
              Build AI Company
            </Button>
          </div>
        }
      />

      <Tabs defaultValue="agents">
        <TabsList className="bg-white/[0.03] border border-white/[0.08] mb-6">
          <TabsTrigger value="agents" className="gap-1.5 text-xs">
            <Bot className="w-3.5 h-3.5" />
            Agents ({agents.length})
          </TabsTrigger>
          <TabsTrigger value="skills" className="gap-1.5 text-xs">
            <Sparkles className="w-3.5 h-3.5" />
            Skill Library ({allSkills.length})
          </TabsTrigger>
        </TabsList>

        {/* ============================================================ */}
        {/*  Agents Tab                                                   */}
        {/* ============================================================ */}
        <TabsContent value="agents">
          {agents.length === 0 ? (
            <EmptyState
              icon={Bot}
              title="No agents yet"
              description="Describe what you want to automate and Nexus will design a team of specialized agents — or compose one manually in the workflow builder."
              action={
                <div className="flex items-center gap-2 flex-wrap justify-center">
                  <Button
                    onClick={() => router.push(`/${projectId}/agents/create`)}
                    className="gap-2 bg-gradient-to-r from-glow-cyan to-glow-blue text-white"
                  >
                    <Sparkles className="w-4 h-4" />
                    Build AI Company
                  </Button>
                  <Button
                    variant="outline"
                    onClick={() => setQuickCreateOpen(true)}
                    className="gap-2"
                  >
                    <Plus className="w-4 h-4" />
                    Quick Create from Preset
                  </Button>
                  <Button
                    variant="outline"
                    onClick={() => router.push(`/${projectId}/workflows/compose`)}
                    className="gap-2"
                  >
                    <Network className="w-4 h-4" />
                    Compose Workflow
                  </Button>
                </div>
              }
            />
          ) : (
            <div className="space-y-3">
              {agents.map((agent) => {
                const isExpanded = expanded === agent.id;
                const assignedSkills = agentSkills[agent.id] ?? [];
                const assignedIds = new Set(assignedSkills.map((s) => s.id));

                return (
                  <Card key={agent.id} className="glass-card-hover overflow-hidden">
                    {/* Agent header */}
                    <div
                      className="flex items-center gap-4 p-4 cursor-pointer hover:bg-white/[0.02] transition-colors"
                      onClick={() => setExpanded(isExpanded ? null : agent.id)}
                    >
                      <div className="relative">
                        <div className="w-10 h-10 rounded-xl bg-white/[0.05] border border-white/[0.08] flex items-center justify-center">
                          <Bot className="w-5 h-5 text-glow-cyan" />
                        </div>
                        <span
                          className={cn(
                            "absolute -bottom-0.5 -right-0.5 w-2.5 h-2.5 rounded-full border-2 border-background",
                            STATUS_DOT[agent.status] ?? "bg-slate-600"
                          )}
                        />
                      </div>

                      <div className="flex-1 min-w-0">
                        <div className="flex items-center gap-2 mb-0.5">
                          <span className="font-semibold text-sm">{agent.name}</span>
                          <Badge variant={STATUS_BADGE_VARIANT[agent.status] ?? "secondary"}>
                            {agent.status}
                          </Badge>
                          {assignedSkills.length > 0 && (
                            <Badge variant="info" className="text-[10px] h-4 px-1.5 gap-0.5">
                              <Zap className="w-2.5 h-2.5" />
                              {assignedSkills.length} skill{assignedSkills.length !== 1 ? "s" : ""}
                            </Badge>
                          )}
                        </div>
                        <p className="text-xs text-slate-400 truncate">{agent.role}</p>
                      </div>

                      <Badge variant="outline" className="text-[10px] font-mono">
                        {agent.model}
                      </Badge>

                      {agent.tools.length > 0 && (
                        <div className="flex items-center gap-1 text-xs text-slate-400">
                          <Wrench className="w-3 h-3" />
                          {agent.tools.length}
                        </div>
                      )}

                      <div className="flex items-center gap-1 ml-2" onClick={(e) => e.stopPropagation()}>
                        <Button
                          variant="ghost"
                          size="icon"
                          onClick={() => toggleRun(agent)}
                          className={cn(
                            "h-7 w-7",
                            agent.status === "running"
                              ? "text-destructive hover:text-destructive hover:bg-destructive/10"
                              : "text-emerald-400 hover:text-emerald-400 hover:bg-emerald-500/10"
                          )}
                        >
                          {agent.status === "running" ? (
                            <Square className="w-3 h-3" />
                          ) : (
                            <Play className="w-3 h-3" />
                          )}
                        </Button>
                        <Button
                          variant="ghost"
                          size="icon"
                          onClick={() => remove(agent.id)}
                          className="h-7 w-7 text-slate-400 hover:text-destructive hover:bg-destructive/10"
                        >
                          <Trash2 className="w-3 h-3" />
                        </Button>
                      </div>

                      {isExpanded ? (
                        <ChevronUp className="w-4 h-4 text-slate-400" />
                      ) : (
                        <ChevronDown className="w-4 h-4 text-slate-400" />
                      )}
                    </div>

                    {/* Expanded details */}
                    {isExpanded && (
                      <>
                        <Separator />
                        <CardContent className="pt-4 pb-4 space-y-6">
                          {/* Settings row */}
                          <div className="grid grid-cols-1 lg:grid-cols-3 gap-4">
                            <div>
                              <p className="text-[11px] font-medium text-slate-400 uppercase tracking-wider mb-2">
                                <Cpu className="w-3 h-3 inline mr-1" />
                                LLM Model
                              </p>
                              <ModelSelector
                                selectedProvider={agent.provider}
                                selectedModel={agent.model}
                                onChange={async (provider, model) => {
                                  try {
                                    await api.updateAgentModel(projectId, agent.id, provider, model);
                                    await loadAgents();
                                  } catch { /* ignore */ }
                                }}
                              />
                            </div>
                            <div>
                              <p className="text-[11px] font-medium text-slate-400 uppercase tracking-wider mb-2">
                                Memory
                              </p>
                              <p className="text-sm">{agent.memory_type}</p>
                              {agent.zeroclaw_pid && (
                                <p className="text-xs text-slate-400 font-mono mt-1">
                                  PID: {agent.zeroclaw_pid}
                                </p>
                              )}
                            </div>
                            <div>
                              <p className="text-[11px] font-medium text-slate-400 uppercase tracking-wider mb-2">
                                Composed Prompt
                              </p>
                              <Button
                                variant="outline"
                                size="sm"
                                className="gap-1.5 text-xs"
                                onClick={() => handlePreviewPrompt(agent.id, agent.name)}
                              >
                                <Eye className="w-3 h-3" />
                                Preview Full Prompt
                              </Button>
                            </div>
                          </div>

                          {/* Tools */}
                          {agent.tools.length > 0 && (
                            <div>
                              <p className="text-[11px] font-medium text-slate-400 uppercase tracking-wider mb-2">
                                Tools
                              </p>
                              <div className="flex flex-wrap gap-1.5">
                                {agent.tools.map((t) => (
                                  <Badge key={t} variant="info">
                                    {t}
                                  </Badge>
                                ))}
                              </div>
                            </div>
                          )}

                          {/* Active skills */}
                          <div>
                            <p className="text-[11px] font-medium text-slate-400 uppercase tracking-wider mb-3">
                              <Sparkles className="w-3 h-3 inline mr-1" />
                              Super Skills ({assignedSkills.length} active)
                            </p>
                            {assignedSkills.length > 0 ? (
                              <div className="flex flex-wrap gap-2 mb-4">
                                {assignedSkills.map((s) => {
                                  return (
                                    <div
                                      key={s.id}
                                      className="flex items-center gap-2 px-3 py-1.5 rounded-lg bg-glow-cyan/[0.05] border border-primary/20"
                                    >
                                      <span className="text-sm">{s.icon}</span>
                                      <span className="text-xs font-medium">{s.name}</span>
                                      <Button
                                        variant="ghost"
                                        size="icon"
                                        className="h-5 w-5 text-red-400 hover:text-red-400"
                                        onClick={() => handleUnassignSkill(agent.id, s.id)}
                                      >
                                        <X className="w-3 h-3" />
                                      </Button>
                                    </div>
                                  );
                                })}
                              </div>
                            ) : (
                              <p className="text-xs text-slate-400 mb-4">
                                No skills assigned. Add skills below to enhance this agent.
                              </p>
                            )}

                            {/* Available skills to add */}
                            <div className="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 gap-3">
                              {allSkills
                                .filter((s) => !assignedIds.has(s.id))
                                .slice(0, 6)
                                .map((skill) => (
                                  <SkillCard
                                    key={skill.id}
                                    skill={skill}
                                    assigned={false}
                                    onAssign={() => handleAssignSkill(agent.id, skill.id)}
                                    onUnassign={() => {}}
                                    onPreview={() => handlePreviewSkill(skill)}
                                    loading={!!skillLoading[`${agent.id}-${skill.id}`]}
                                  />
                                ))}
                            </div>
                            {allSkills.filter((s) => !assignedIds.has(s.id)).length > 6 && (
                              <p className="text-xs text-slate-400 mt-2">
                                +{allSkills.filter((s) => !assignedIds.has(s.id)).length - 6} more
                                skills available in the Skill Library tab
                              </p>
                            )}
                          </div>

                          {/* Base system prompt */}
                          <div>
                            <p className="text-[11px] font-medium text-slate-400 uppercase tracking-wider mb-1">
                              Base System Prompt
                            </p>
                            <div className="bg-white/[0.03] border border-white/[0.06] rounded-lg p-3 font-mono text-xs text-slate-400 line-clamp-4 leading-relaxed">
                              {agent.system_prompt}
                            </div>
                          </div>
                        </CardContent>
                      </>
                    )}
                  </Card>
                );
              })}
            </div>
          )}
        </TabsContent>

        {/* ============================================================ */}
        {/*  Skill Library Tab                                            */}
        {/* ============================================================ */}
        <TabsContent value="skills">
          {/* Category filter */}
          <div className="flex items-center gap-2 mb-6 overflow-x-auto scrollbar-thin pb-2">
            {categories.map((cat) => {
              const count = cat === "all" ? allSkills.length : allSkills.filter((s) => s.category === cat).length;
              return (
                <Button
                  key={cat}
                  variant={skillFilter === cat ? "default" : "outline"}
                  size="sm"
                  className="gap-1.5 text-xs shrink-0 h-8"
                  onClick={() => setSkillFilter(cat)}
                >
                  {cat === "all" ? "All" : cat.charAt(0).toUpperCase() + cat.slice(1)}
                  <Badge variant="secondary" className="text-[10px] h-4 px-1 ml-1">
                    {count}
                  </Badge>
                </Button>
              );
            })}
          </div>

          {/* Skills grid */}
          <div className="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 gap-4">
            {filteredSkills.map((skill) => {
              const cat = getCategoryConfig(skill.category);
              const Icon = cat.icon;

              return (
                <Card key={skill.id} className="glass-card-hover overflow-hidden">
                  <CardContent className="p-5">
                    <div className="flex items-start gap-3 mb-4">
                      <div
                        className={cn(
                          "w-12 h-12 rounded-xl flex items-center justify-center border shrink-0",
                          cat.bg,
                          "border-white/[0.08]"
                        )}
                      >
                        <span className="text-2xl">{skill.icon}</span>
                      </div>
                      <div className="flex-1 min-w-0">
                        <div className="flex items-center gap-2 mb-1">
                          <span className="font-bold text-sm">{skill.name}</span>
                          {skill.is_builtin && (
                            <Badge variant="outline" className="text-[9px] h-4 px-1.5">
                              Built-in
                            </Badge>
                          )}
                        </div>
                        <Badge variant="secondary" className="text-[10px] h-4 gap-1">
                          <Icon className="w-2.5 h-2.5" />
                          {skill.category}
                        </Badge>
                      </div>
                    </div>

                    <p className="text-xs text-slate-400 mb-4 line-clamp-3">
                      {skill.description}
                    </p>

                    {/* Tools */}
                    {skill.tools.length > 0 && (
                      <div className="mb-3">
                        <p className="text-[10px] text-slate-400 uppercase tracking-wider mb-1.5">
                          Tools
                        </p>
                        <div className="flex flex-wrap gap-1">
                          {skill.tools.slice(0, 6).map((t) => (
                            <Badge key={t} variant="secondary" className="text-[10px] h-5">
                              {t}
                            </Badge>
                          ))}
                          {skill.tools.length > 6 && (
                            <Badge variant="secondary" className="text-[10px] h-5">
                              +{skill.tools.length - 6}
                            </Badge>
                          )}
                        </div>
                      </div>
                    )}

                    {/* Rules count */}
                    {skill.rules.length > 0 && (
                      <div className="mb-4">
                        <p className="text-[10px] text-slate-400 uppercase tracking-wider mb-1">
                          {skill.rules.length} Rules
                        </p>
                        <div className="space-y-0.5">
                          {skill.rules.slice(0, 3).map((r, i) => (
                            <p key={i} className="text-[11px] text-slate-400/70 truncate">
                              {i + 1}. {r}
                            </p>
                          ))}
                          {skill.rules.length > 3 && (
                            <p className="text-[10px] text-slate-400/50">
                              +{skill.rules.length - 3} more...
                            </p>
                          )}
                        </div>
                      </div>
                    )}

                    {/* Temperature + Max tokens */}
                    <div className="flex items-center gap-4 mb-4 text-[10px] text-slate-400">
                      <span>Temp: {skill.temperature}</span>
                      <span>Max tokens: {skill.max_tokens.toLocaleString()}</span>
                    </div>

                    {/* Actions */}
                    <div className="flex items-center gap-2">
                      <Button
                        variant="outline"
                        size="sm"
                        className="gap-1.5 text-xs flex-1"
                        onClick={() => handlePreviewSkill(skill)}
                      >
                        <Eye className="w-3 h-3" />
                        View Prompt
                      </Button>
                    </div>
                  </CardContent>
                </Card>
              );
            })}
          </div>

          {filteredSkills.length === 0 && (
            <EmptyState
              icon={Sparkles}
              title="No skills in this category"
              description="Try selecting a different category or create a custom skill."
            />
          )}
        </TabsContent>
      </Tabs>

      {/* Prompt preview dialog */}
      <PromptPreviewDialog
        open={promptPreview.open}
        onClose={() => setPromptPreview({ open: false, content: "", title: "" })}
        content={promptPreview.content}
        title={promptPreview.title}
      />

      {/* Quick create agent dialog */}
      <QuickCreateAgentDialog
        projectId={projectId}
        open={quickCreateOpen}
        onClose={() => setQuickCreateOpen(false)}
        onCreated={loadAgents}
      />
    </div>
  );
}
