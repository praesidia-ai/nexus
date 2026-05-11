"use client";

import { useState } from "react";
import {
  Sparkles,
  Code2,
  Cloud,
  Search,
  BookOpen,
  Shield,
  BarChart3,
  Megaphone,
  Server,
  ClipboardList,
  HeadphonesIcon,
  Loader2,
  Check,
  type LucideIcon,
} from "lucide-react";
import { cn } from "@/lib/utils";
import { api } from "@/lib/api";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
} from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Badge } from "@/components/ui/badge";
import { ScrollArea } from "@/components/ui/scroll-area";

interface RosterPreset {
  id: string;
  name: string;
  icon: LucideIcon;
  accent: string;
  domain: string;
  tagline: string;
  system_prompt_seed: string;
  default_tools: string[];
}

const ROSTER: RosterPreset[] = [
  {
    id: "nova",
    name: "Nova",
    icon: Code2,
    accent: "text-blue-400 bg-blue-500/10 border-blue-500/20",
    domain: "Software Engineering",
    tagline: "Full-stack coder: architecture, implementation, tests, refactoring",
    system_prompt_seed:
      "You are Nova, an expert software engineer. You write clean, production-ready code in any language. You scaffold projects, implement features, write tests, and refactor. Always prefer simplicity, correctness, and idiomatic patterns.",
    default_tools: ["shell", "file", "git", "web_fetch"],
  },
  {
    id: "atlas",
    name: "Atlas",
    icon: Cloud,
    accent: "text-cyan-400 bg-cyan-500/10 border-cyan-500/20",
    domain: "Cloud & Infrastructure",
    tagline: "Cloud architect: AWS/GCP/Azure, IaC, containers, scaling",
    system_prompt_seed:
      "You are Atlas, a cloud and infrastructure expert. You design and implement cloud architectures, write Terraform/Pulumi IaC, configure Kubernetes, and optimize cost and reliability.",
    default_tools: ["shell", "file", "web_fetch"],
  },
  {
    id: "kai",
    name: "Kai",
    icon: Search,
    accent: "text-purple-400 bg-purple-500/10 border-purple-500/20",
    domain: "Research",
    tagline: "Researcher: deep web research, competitive analysis, synthesis",
    system_prompt_seed:
      "You are Kai, a meticulous researcher. You gather information from multiple sources, synthesize findings, and produce clear, well-cited summaries and recommendations.",
    default_tools: ["web_fetch", "web_search", "file"],
  },
  {
    id: "luna",
    name: "Luna",
    icon: BookOpen,
    accent: "text-emerald-400 bg-emerald-500/10 border-emerald-500/20",
    domain: "Technical Writing",
    tagline: "Writer: READMEs, API docs, user guides, blog posts",
    system_prompt_seed:
      "You are Luna, a skilled technical writer. You produce clear, accurate, and engaging documentation, READMEs, API references, tutorials, and blog posts.",
    default_tools: ["file", "web_fetch"],
  },
  {
    id: "orion",
    name: "Orion",
    icon: Shield,
    accent: "text-red-400 bg-red-500/10 border-red-500/20",
    domain: "Security",
    tagline: "Security auditor: OWASP, threat modelling, pen-test, hardening",
    system_prompt_seed:
      "You are Orion, a security specialist. You audit code and infrastructure for vulnerabilities, model threats, recommend mitigations, and verify fixes against security best-practices.",
    default_tools: ["shell", "file", "web_fetch"],
  },
  {
    id: "sage",
    name: "Sage",
    icon: BarChart3,
    accent: "text-amber-400 bg-amber-500/10 border-amber-500/20",
    domain: "Data & Analytics",
    tagline: "Data analyst: SQL, ETL, visualisation, ML pipelines",
    system_prompt_seed:
      "You are Sage, a data and analytics expert. You design data models, write SQL, build ETL pipelines, create visualisations, and prototype ML workflows.",
    default_tools: ["shell", "file", "web_fetch"],
  },
  {
    id: "ivy",
    name: "Ivy",
    icon: Megaphone,
    accent: "text-pink-400 bg-pink-500/10 border-pink-500/20",
    domain: "Marketing",
    tagline: "Marketer: copy, SEO, campaigns, positioning, growth",
    system_prompt_seed:
      "You are Ivy, a marketing strategist and copywriter. You craft compelling messaging, SEO-optimised content, campaign plans, and growth strategies.",
    default_tools: ["web_fetch", "web_search", "file"],
  },
  {
    id: "rex",
    name: "Rex",
    icon: Server,
    accent: "text-orange-400 bg-orange-500/10 border-orange-500/20",
    domain: "DevOps",
    tagline: "DevOps engineer: CI/CD, Docker, monitoring, deployment",
    system_prompt_seed:
      "You are Rex, a DevOps engineer. You configure CI/CD pipelines, write Dockerfiles and Compose files, set up monitoring and alerting, and automate deployments.",
    default_tools: ["shell", "file", "git", "web_fetch"],
  },
  {
    id: "leo",
    name: "Leo",
    icon: ClipboardList,
    accent: "text-indigo-400 bg-indigo-500/10 border-indigo-500/20",
    domain: "Product Management",
    tagline: "PM: requirements, user stories, roadmaps, prioritisation",
    system_prompt_seed:
      "You are Leo, a product manager. You gather and refine requirements, write user stories, build roadmaps, and keep the team focused on user value.",
    default_tools: ["file", "web_fetch"],
  },
  {
    id: "mia",
    name: "Mia",
    icon: HeadphonesIcon,
    accent: "text-teal-400 bg-teal-500/10 border-teal-500/20",
    domain: "Customer Support",
    tagline: "Support specialist: FAQs, escalation paths, empathy-first comms",
    system_prompt_seed:
      "You are Mia, a customer support specialist. You handle user queries with empathy and precision, write help-centre articles, and design support workflows.",
    default_tools: ["file", "web_fetch"],
  },
];

interface QuickCreateAgentDialogProps {
  projectId: string;
  open: boolean;
  onClose: () => void;
  onCreated: () => void;
}

export function QuickCreateAgentDialog({
  projectId,
  open,
  onClose,
  onCreated,
}: QuickCreateAgentDialogProps) {
  const [selected, setSelected] = useState<RosterPreset | null>(null);
  const [name, setName] = useState("");
  const [role, setRole] = useState("");
  const [systemPrompt, setSystemPrompt] = useState("");
  const [toolsInput, setToolsInput] = useState("");
  const [creating, setCreating] = useState(false);
  const [error, setError] = useState<string | null>(null);

  function pickPreset(preset: RosterPreset) {
    setSelected(preset);
    setName(preset.name);
    setRole(preset.tagline);
    setSystemPrompt(preset.system_prompt_seed);
    setToolsInput(preset.default_tools.join(", "));
    setError(null);
  }

  function reset() {
    setSelected(null);
    setName("");
    setRole("");
    setSystemPrompt("");
    setToolsInput("");
    setError(null);
  }

  async function handleCreate() {
    setError(null);
    setCreating(true);
    try {
      await api.createAgent(projectId, {
        name: name.trim(),
        role: role.trim(),
        tools: toolsInput
          .split(",")
          .map((t) => t.trim())
          .filter(Boolean),
        system_prompt: systemPrompt.trim(),
      });
      reset();
      onCreated();
      onClose();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to create agent");
    } finally {
      setCreating(false);
    }
  }

  const canCreate =
    name.trim().length > 0 &&
    role.trim().length > 0 &&
    systemPrompt.trim().length > 0 &&
    !creating;

  return (
    <Dialog
      open={open}
      onOpenChange={(v) => {
        if (!v) {
          reset();
          onClose();
        }
      }}
    >
      <DialogContent className="max-w-4xl max-h-[85vh] overflow-hidden flex flex-col">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <Sparkles className="w-4 h-4 text-glow-cyan" />
            Quick Create Agent
          </DialogTitle>
          <DialogDescription>
            Pick a ZeroClaw roster preset or start from scratch. Edit the
            details below, then deploy.
          </DialogDescription>
        </DialogHeader>

        <div className="grid grid-cols-1 lg:grid-cols-5 gap-4 flex-1 min-h-0">
          {/* Preset list */}
          <div className="lg:col-span-2 flex flex-col min-h-0">
            <p className="text-[11px] font-medium text-slate-400 uppercase tracking-wider mb-2">
              ZeroClaw Roster
            </p>
            <ScrollArea className="flex-1 pr-2">
              <div className="space-y-1.5">
                {ROSTER.map((preset) => {
                  const Icon = preset.icon;
                  const isSelected = selected?.id === preset.id;
                  return (
                    <button
                      key={preset.id}
                      onClick={() => pickPreset(preset)}
                      className={cn(
                        "w-full text-left p-3 rounded-lg border transition-all",
                        isSelected
                          ? "border-glow-cyan/30 bg-glow-cyan/[0.05] shadow-[0_0_20px_rgba(0,200,255,0.06)]"
                          : "border-white/[0.06] bg-white/[0.02] hover:bg-white/[0.04] hover:border-white/[0.12]",
                      )}
                    >
                      <div className="flex items-start gap-2.5">
                        <div
                          className={cn(
                            "w-9 h-9 rounded-lg flex items-center justify-center border shrink-0",
                            preset.accent,
                          )}
                        >
                          <Icon className="w-4 h-4" />
                        </div>
                        <div className="flex-1 min-w-0">
                          <div className="flex items-center gap-1.5 mb-0.5">
                            <span className="font-semibold text-sm">
                              {preset.name}
                            </span>
                            {isSelected && (
                              <Check className="w-3 h-3 text-glow-cyan" />
                            )}
                          </div>
                          <p className="text-[11px] text-slate-400 line-clamp-2">
                            {preset.tagline}
                          </p>
                        </div>
                      </div>
                    </button>
                  );
                })}
              </div>
            </ScrollArea>
          </div>

          {/* Editor */}
          <div className="lg:col-span-3 flex flex-col min-h-0 space-y-3 overflow-y-auto pr-1">
            <div className="grid grid-cols-2 gap-3">
              <div>
                <label className="text-[11px] font-medium text-slate-400 uppercase tracking-wider mb-1.5 block">
                  Name
                </label>
                <Input
                  value={name}
                  onChange={(e) => setName(e.target.value)}
                  placeholder="e.g. Nova, Researcher-01"
                  className="bg-white/[0.03] border-white/[0.08]"
                />
              </div>
              <div>
                <label className="text-[11px] font-medium text-slate-400 uppercase tracking-wider mb-1.5 block">
                  Role
                </label>
                <Input
                  value={role}
                  onChange={(e) => setRole(e.target.value)}
                  placeholder="Short description"
                  className="bg-white/[0.03] border-white/[0.08]"
                />
              </div>
            </div>

            <div>
              <label className="text-[11px] font-medium text-slate-400 uppercase tracking-wider mb-1.5 block">
                System Prompt
              </label>
              <textarea
                value={systemPrompt}
                onChange={(e) => setSystemPrompt(e.target.value)}
                rows={6}
                placeholder="Describe the agent's personality, expertise, and behavior..."
                className="w-full bg-white/[0.03] border border-white/[0.08] rounded-lg p-3 text-sm text-white placeholder:text-slate-500 resize-none focus:outline-none focus:border-glow-cyan/30"
              />
            </div>

            <div>
              <label className="text-[11px] font-medium text-slate-400 uppercase tracking-wider mb-1.5 block">
                Tools (comma-separated)
              </label>
              <Input
                value={toolsInput}
                onChange={(e) => setToolsInput(e.target.value)}
                placeholder="shell, file, git, web_fetch"
                className="bg-white/[0.03] border-white/[0.08] font-mono text-xs"
              />
              {toolsInput.trim().length > 0 && (
                <div className="flex flex-wrap gap-1 mt-2">
                  {toolsInput
                    .split(",")
                    .map((t) => t.trim())
                    .filter(Boolean)
                    .map((t) => (
                      <Badge
                        key={t}
                        variant="secondary"
                        className="text-[10px] h-5"
                      >
                        {t}
                      </Badge>
                    ))}
                </div>
              )}
            </div>

            {error && (
              <div className="rounded-lg border border-red-500/20 bg-red-500/10 px-3 py-2 text-xs text-red-400">
                {error}
              </div>
            )}
          </div>
        </div>

        <div className="flex items-center justify-between pt-4 border-t border-white/[0.06]">
          <p className="text-[11px] text-slate-400">
            {selected
              ? `Based on ${selected.name} · ${selected.domain}`
              : "Start from scratch"}
          </p>
          <div className="flex items-center gap-2">
            <Button variant="outline" size="sm" onClick={onClose}>
              Cancel
            </Button>
            <Button
              size="sm"
              onClick={handleCreate}
              disabled={!canCreate}
              className="gap-1.5 bg-gradient-to-r from-glow-cyan to-glow-blue text-white hover:brightness-110"
            >
              {creating ? (
                <Loader2 className="w-3.5 h-3.5 animate-spin" />
              ) : (
                <Sparkles className="w-3.5 h-3.5" />
              )}
              Create Agent
            </Button>
          </div>
        </div>
      </DialogContent>
    </Dialog>
  );
}
