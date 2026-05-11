"use client";

import { useState, useEffect, useCallback } from "react";
import { useParams, useRouter, useSearchParams } from "next/navigation";
import {
  Plus,
  Trash2,
  Play,
  Loader2,
  ArrowLeft,
  ArrowRight,
  Zap,
  MessageSquare,
  Crown,
  Shuffle,
  Users,
  DollarSign,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { PageHeader } from "@/components/page-header";
import { useToast } from "@/components/toast";
import { teamApi, type TeamTemplate, type TeamMemberConfig } from "@/lib/team-api";

// ---------------------------------------------------------------------------
// Protocol definitions
// ---------------------------------------------------------------------------

const PROTOCOLS = [
  { id: "sequential", label: "Sequential", icon: ArrowRight, color: "text-blue-400", description: "Members work one after another, passing results forward." },
  { id: "parallel", label: "Parallel", icon: Zap, color: "text-emerald-400", description: "Members work simultaneously, results merged at the end." },
  { id: "debate", label: "Debate", icon: MessageSquare, color: "text-amber-400", description: "Members argue and critique to reach the best solution." },
  { id: "hierarchical", label: "Hierarchical", icon: Crown, color: "text-purple-400", description: "A lead member coordinates and delegates to others." },
  { id: "swarm", label: "Swarm", icon: Shuffle, color: "text-rose-400", description: "Dynamic self-organizing coordination between members." },
] as const;

const MODELS = [
  "claude-sonnet-4-6",
  "claude-opus-4-20250514",
  "gpt-5.4-mini",
  "gpt-5.4-nano",
  "o3-mini",
];

const AVAILABLE_TOOLS = [
  "code_write",
  "code_analysis",
  "web_search",
  "browser",
  "data_analysis",
  "database",
  "test_runner",
  "deploy",
  "security_scan",
  "file_system",
];

function makeEmptyMember(): TeamMemberConfig {
  return {
    name: "",
    role: "",
    model: "claude-sonnet-4-6",
    tools: [],
    systemPrompt: "",
  };
}

// ---------------------------------------------------------------------------
// Page
// ---------------------------------------------------------------------------

export default function TeamBuilderPage() {
  const params = useParams();
  const router = useRouter();
  const searchParams = useSearchParams();
  const projectId = params.projectId as string;
  const templateId = searchParams.get("template");
  const { toast } = useToast();

  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  const [protocol, setProtocol] = useState("sequential");
  const [members, setMembers] = useState<TeamMemberConfig[]>([makeEmptyMember()]);
  const [budget, setBudget] = useState(5.0);
  const [maxDuration, setMaxDuration] = useState(600);
  const [creating, setCreating] = useState(false);
  const [loadingTemplate, setLoadingTemplate] = useState(!!templateId);

  // Load template if provided
  useEffect(() => {
    if (!templateId) return;
    setLoadingTemplate(true);
    teamApi
      .listTemplates()
      .then((templates) => {
        const template = templates.find((t) => t.id === templateId);
        if (template) {
          applyTemplate(template);
        }
      })
      .catch(() => {})
      .finally(() => setLoadingTemplate(false));
  }, [templateId]);

  const applyTemplate = (template: TeamTemplate) => {
    setName(template.name ?? "");
    setDescription(template.description ?? "");
    setProtocol(template.protocol ?? "direct");
    setMembers(
      (template.members ?? []).map((m) => ({
        name: m.name ?? "",
        role: m.role ?? "",
        model: m.model ?? "claude-sonnet-4-6",
        tools: m.tools ?? [],
        systemPrompt: m.systemPrompt ?? "",
      }))
    );
    setBudget(Math.max((template.estimatedCostPerRun ?? 1) * 3, 5));
  };

  const addMember = useCallback(() => {
    setMembers((prev) => [...prev, makeEmptyMember()]);
  }, []);

  const removeMember = useCallback((index: number) => {
    setMembers((prev) => prev.filter((_, i) => i !== index));
  }, []);

  const updateMember = useCallback(
    (index: number, updates: Partial<TeamMemberConfig>) => {
      setMembers((prev) =>
        prev.map((m, i) => (i === index ? { ...m, ...updates } : m))
      );
    },
    []
  );

  const toggleTool = useCallback((index: number, tool: string) => {
    setMembers((prev) =>
      prev.map((m, i) => {
        if (i !== index) return m;
        const tools = m.tools.includes(tool)
          ? m.tools.filter((t) => t !== tool)
          : [...m.tools, tool];
        return { ...m, tools };
      })
    );
  }, []);

  const handleCreate = async () => {
    if (!name.trim()) {
      toast("error", "Team name is required");
      return;
    }
    if (members.length === 0) {
      toast("error", "Add at least one member");
      return;
    }
    const invalidMembers = members.filter((m) => !m.name.trim() || !m.role.trim());
    if (invalidMembers.length > 0) {
      toast("error", "All members need a name and role");
      return;
    }

    setCreating(true);
    try {
      const team = await teamApi.createTeam({
        name,
        description,
        templateId: templateId ?? undefined,
        protocol,
        members,
        budget,
        maxDuration,
      });
      toast("success", "Team created", `${team.name} is ready to run`);
      router.push(`/${projectId}/teams/${team.id}/runs/new`);
    } catch (err) {
      toast("error", "Failed to create team", (err as Error).message);
    } finally {
      setCreating(false);
    }
  };

  if (loadingTemplate) {
    return (
      <div className="flex items-center justify-center h-full">
        <Loader2 className="w-6 h-6 animate-spin text-slate-400" />
      </div>
    );
  }

  return (
    <div className="h-full overflow-y-auto scrollbar-thin">
      <div className="max-w-4xl mx-auto px-6 py-6 space-y-8">
        <PageHeader
          title="Build a Team"
          subtitle="Configure members, protocol, and budget"
          actions={
            <Button
              variant="ghost"
              onClick={() => router.push(`/${projectId}/teams`)}
              className="text-slate-400"
            >
              <ArrowLeft className="w-4 h-4 mr-2" />
              Back to Gallery
            </Button>
          }
        />

        {/* Team name and description */}
        <div className="space-y-4">
          <div>
            <label className="text-[11px] text-slate-400 uppercase tracking-wider block mb-1.5">
              Team Name
            </label>
            <input
              type="text"
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder="e.g., Full-Stack Build Team"
              className="w-full text-sm bg-white/[0.03] border border-white/[0.08] rounded-lg px-3 py-2 text-slate-200 placeholder:text-slate-400/50 focus:outline-none focus:border-white/20"
            />
          </div>
          <div>
            <label className="text-[11px] text-slate-400 uppercase tracking-wider block mb-1.5">
              Description
            </label>
            <textarea
              value={description}
              onChange={(e) => setDescription(e.target.value)}
              placeholder="What does this team do?"
              rows={2}
              className="w-full text-sm bg-white/[0.03] border border-white/[0.08] rounded-lg px-3 py-2 text-slate-200 placeholder:text-slate-400/50 focus:outline-none focus:border-white/20 resize-none"
            />
          </div>
        </div>

        {/* Protocol selector */}
        <div>
          <label className="text-[11px] text-slate-400 uppercase tracking-wider block mb-3">
            Coordination Protocol
          </label>
          <div className="grid grid-cols-5 gap-2">
            {PROTOCOLS.map((p) => {
              const Icon = p.icon;
              return (
                <button
                  key={p.id}
                  onClick={() => setProtocol(p.id)}
                  className={`flex flex-col items-center gap-2 p-4 rounded-xl border transition-all ${
                    protocol === p.id
                      ? "border-glow-cyan/30 bg-glow-cyan/[0.06]"
                      : "border-white/[0.06] bg-white/[0.02] hover:bg-white/[0.04]"
                  }`}
                >
                  <Icon
                    className={`w-5 h-5 ${
                      protocol === p.id ? "text-glow-cyan" : p.color
                    }`}
                  />
                  <span
                    className={`text-xs font-medium ${
                      protocol === p.id ? "text-glow-cyan" : "text-slate-200"
                    }`}
                  >
                    {p.label}
                  </span>
                  <span className="text-[10px] text-slate-400 text-center leading-tight">
                    {p.description}
                  </span>
                </button>
              );
            })}
          </div>
        </div>

        {/* Members */}
        <div>
          <div className="flex items-center justify-between mb-3">
            <label className="text-[11px] text-slate-400 uppercase tracking-wider">
              Team Members
            </label>
            <Button variant="ghost" size="sm" onClick={addMember} className="text-xs">
              <Plus className="w-3 h-3 mr-1" />
              Add Member
            </Button>
          </div>
          <div className="space-y-3">
            {members.map((member, index) => (
              <div
                key={index}
                className="p-4 rounded-xl border border-white/[0.06] bg-white/[0.02] space-y-3"
              >
                <div className="flex items-center gap-3">
                  <div className="w-8 h-8 rounded-lg bg-glow-cyan/10 flex items-center justify-center flex-shrink-0">
                    <span className="text-glow-cyan text-xs font-bold">
                      {member.name ? member.name[0].toUpperCase() : (index + 1).toString()}
                    </span>
                  </div>
                  <div className="flex-1 grid grid-cols-3 gap-2">
                    <input
                      type="text"
                      value={member.name}
                      onChange={(e) => updateMember(index, { name: e.target.value })}
                      placeholder="Name"
                      className="text-sm bg-white/[0.03] border border-white/[0.08] rounded-md px-2.5 py-1.5 text-slate-200 placeholder:text-slate-400/50 focus:outline-none focus:border-white/20"
                    />
                    <input
                      type="text"
                      value={member.role}
                      onChange={(e) => updateMember(index, { role: e.target.value })}
                      placeholder="Role"
                      className="text-sm bg-white/[0.03] border border-white/[0.08] rounded-md px-2.5 py-1.5 text-slate-200 placeholder:text-slate-400/50 focus:outline-none focus:border-white/20"
                    />
                    <select
                      value={member.model}
                      onChange={(e) => updateMember(index, { model: e.target.value })}
                      className="text-sm bg-white/[0.03] border border-white/[0.08] rounded-md px-2.5 py-1.5 text-slate-200 focus:outline-none focus:border-white/20"
                    >
                      {MODELS.map((m) => (
                        <option key={m} value={m}>
                          {m}
                        </option>
                      ))}
                    </select>
                  </div>
                  <button
                    onClick={() => removeMember(index)}
                    className="text-slate-400 hover:text-destructive transition-colors p-1"
                    disabled={members.length <= 1}
                  >
                    <Trash2 className="w-3.5 h-3.5" />
                  </button>
                </div>
                {/* Tools */}
                <div>
                  <label className="text-[10px] text-slate-400 uppercase tracking-wider block mb-1.5">
                    Tools
                  </label>
                  <div className="flex flex-wrap gap-1.5">
                    {AVAILABLE_TOOLS.map((tool) => (
                      <button
                        key={tool}
                        onClick={() => toggleTool(index, tool)}
                        className={`text-[11px] px-2 py-0.5 rounded-md border transition-colors ${
                          member.tools.includes(tool)
                            ? "border-primary/30 bg-glow-cyan/10 text-glow-cyan"
                            : "border-white/[0.06] bg-white/[0.02] text-slate-400 hover:bg-white/[0.05]"
                        }`}
                      >
                        {tool.replace(/_/g, " ")}
                      </button>
                    ))}
                  </div>
                </div>
              </div>
            ))}
          </div>
        </div>

        {/* Budget and duration */}
        <div className="grid grid-cols-2 gap-6">
          <div>
            <label className="text-[11px] text-slate-400 uppercase tracking-wider block mb-2">
              Budget Limit (USD)
            </label>
            <div className="space-y-2">
              <input
                type="range"
                min={0.5}
                max={50}
                step={0.5}
                value={budget}
                onChange={(e) => setBudget(parseFloat(e.target.value))}
                className="w-full accent-cyan-500"
              />
              <div className="flex items-center justify-between text-xs text-slate-400">
                <span>$0.50</span>
                <span className="text-slate-200 font-medium text-sm">
                  ${budget.toFixed(2)}
                </span>
                <span>$50.00</span>
              </div>
            </div>
          </div>
          <div>
            <label className="text-[11px] text-slate-400 uppercase tracking-wider block mb-2">
              Max Duration (seconds)
            </label>
            <div className="space-y-2">
              <input
                type="range"
                min={60}
                max={3600}
                step={60}
                value={maxDuration}
                onChange={(e) => setMaxDuration(parseInt(e.target.value))}
                className="w-full accent-cyan-500"
              />
              <div className="flex items-center justify-between text-xs text-slate-400">
                <span>1 min</span>
                <span className="text-slate-200 font-medium text-sm">
                  {Math.floor(maxDuration / 60)}m {maxDuration % 60}s
                </span>
                <span>60 min</span>
              </div>
            </div>
          </div>
        </div>

        {/* Summary and create */}
        <div className="flex items-center justify-between p-4 rounded-xl border border-white/[0.06] bg-white/[0.02]">
          <div className="flex items-center gap-4">
            <div className="flex items-center gap-1.5">
              <Users className="w-4 h-4 text-slate-400" />
              <span className="text-sm text-slate-200">{members.length} members</span>
            </div>
            <div className="flex items-center gap-1.5">
              {(() => {
                const proto = PROTOCOLS.find((p) => p.id === protocol);
                if (!proto) return null;
                const Icon = proto.icon;
                return (
                  <>
                    <Icon className={`w-4 h-4 ${proto.color}`} />
                    <span className="text-sm text-slate-200">{proto.label}</span>
                  </>
                );
              })()}
            </div>
            <div className="flex items-center gap-1.5">
              <DollarSign className="w-4 h-4 text-slate-400" />
              <span className="text-sm text-slate-200">${budget.toFixed(2)} budget</span>
            </div>
          </div>
          <Button
            onClick={handleCreate}
            disabled={creating || !name.trim()}
            className="bg-gradient-to-r from-glow-cyan to-glow-blue hover:brightness-110 text-white font-semibold shadow-glow-sm"
          >
            {creating ? (
              <>
                <Loader2 className="w-4 h-4 mr-2 animate-spin" />
                Creating...
              </>
            ) : (
              <>
                <Play className="w-4 h-4 mr-2" />
                Create Team
              </>
            )}
          </Button>
        </div>
      </div>
    </div>
  );
}
