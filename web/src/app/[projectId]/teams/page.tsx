"use client";

import { useState, useEffect, useMemo } from "react";
import { useParams, useRouter } from "next/navigation";
import {
  Users,
  Search,
  Plus,
  ArrowRight,
  Loader2,
  Shuffle,
  MessageSquare,
  Crown,
  Zap,
} from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { EmptyState } from "@/components/empty-state";
import { PageHeader } from "@/components/page-header";
import { teamApi, type TeamTemplate } from "@/lib/team-api";

// ---------------------------------------------------------------------------
// Protocol icon map
// ---------------------------------------------------------------------------

const PROTOCOL_ICONS: Record<string, { icon: typeof Users; color: string; label: string }> = {
  sequential: { icon: ArrowRight, color: "text-blue-400", label: "Sequential" },
  parallel: { icon: Zap, color: "text-emerald-400", label: "Parallel" },
  debate: { icon: MessageSquare, color: "text-amber-400", label: "Debate" },
  hierarchical: { icon: Crown, color: "text-purple-400", label: "Hierarchical" },
  swarm: { icon: Shuffle, color: "text-rose-400", label: "Swarm" },
};

const CATEGORIES = ["All", "Engineering", "Research", "Creative", "Operations", "Security"];

// ---------------------------------------------------------------------------
// Fallback templates for when API is empty
// ---------------------------------------------------------------------------

const FALLBACK_TEMPLATES: TeamTemplate[] = [
  {
    id: "tpl-fullstack",
    name: "Full-Stack Build Team",
    description: "Architect, frontend, backend, and QA engineers working together to build a complete application.",
    category: "Engineering",
    protocol: "hierarchical",
    members: [
      { role: "Architect", name: "Atlas", model: "claude-sonnet-4-6", tools: ["code_analysis", "design"] },
      { role: "Frontend Engineer", name: "Nova", model: "claude-sonnet-4-6", tools: ["code_write", "browser"] },
      { role: "Backend Engineer", name: "Rex", model: "claude-sonnet-4-6", tools: ["code_write", "database"] },
      { role: "QA Engineer", name: "Orion", model: "claude-sonnet-4-6", tools: ["test_runner", "code_analysis"] },
    ],
    estimatedCostPerRun: 0.85,
    tags: ["fullstack", "build", "testing"],
  },
  {
    id: "tpl-research",
    name: "Research Synthesis Team",
    description: "Multi-perspective research with parallel investigation and consensus building.",
    category: "Research",
    protocol: "parallel",
    members: [
      { role: "Lead Researcher", name: "Kai", model: "claude-sonnet-4-6", tools: ["web_search", "analysis"] },
      { role: "Data Analyst", name: "Sage", model: "claude-sonnet-4-6", tools: ["data_analysis", "visualization"] },
      { role: "Fact Checker", name: "Orion", model: "claude-sonnet-4-6", tools: ["web_search", "verification"] },
    ],
    estimatedCostPerRun: 0.45,
    tags: ["research", "analysis", "data"],
  },
  {
    id: "tpl-code-review",
    name: "Code Review Board",
    description: "A panel of specialized reviewers that debates code quality, security, and performance.",
    category: "Engineering",
    protocol: "debate",
    members: [
      { role: "Security Reviewer", name: "Orion", model: "claude-sonnet-4-6", tools: ["code_analysis", "security_scan"] },
      { role: "Performance Reviewer", name: "Rex", model: "claude-sonnet-4-6", tools: ["code_analysis", "profiler"] },
      { role: "Style Reviewer", name: "Nova", model: "claude-sonnet-4-6", tools: ["code_analysis", "linter"] },
    ],
    estimatedCostPerRun: 0.35,
    tags: ["review", "security", "quality"],
  },
  {
    id: "tpl-content",
    name: "Content Creation Studio",
    description: "Writer, editor, and SEO specialist collaborate on high-quality content.",
    category: "Creative",
    protocol: "sequential",
    members: [
      { role: "Writer", name: "Luna", model: "claude-sonnet-4-6", tools: ["web_search", "writing"] },
      { role: "Editor", name: "Mia", model: "claude-sonnet-4-6", tools: ["grammar_check", "style_analysis"] },
      { role: "SEO Specialist", name: "Ivy", model: "claude-sonnet-4-6", tools: ["seo_analysis", "keyword_research"] },
    ],
    estimatedCostPerRun: 0.30,
    tags: ["content", "writing", "seo"],
  },
  {
    id: "tpl-incident",
    name: "Incident Response Team",
    description: "Rapid response team for debugging, patching, and deploying fixes.",
    category: "Operations",
    protocol: "swarm",
    members: [
      { role: "Incident Commander", name: "Leo", model: "claude-sonnet-4-6", tools: ["monitoring", "coordination"] },
      { role: "Debugger", name: "Nova", model: "claude-sonnet-4-6", tools: ["code_analysis", "log_search"] },
      { role: "DevOps", name: "Rex", model: "claude-sonnet-4-6", tools: ["deploy", "infrastructure"] },
    ],
    estimatedCostPerRun: 0.50,
    tags: ["incident", "debugging", "devops"],
  },
  {
    id: "tpl-security-audit",
    name: "Security Audit Squad",
    description: "Comprehensive security review with multiple specialized auditors.",
    category: "Security",
    protocol: "parallel",
    members: [
      { role: "Penetration Tester", name: "Orion", model: "claude-sonnet-4-6", tools: ["security_scan", "exploit_check"] },
      { role: "Code Auditor", name: "Nova", model: "claude-sonnet-4-6", tools: ["code_analysis", "dependency_check"] },
      { role: "Infrastructure Auditor", name: "Atlas", model: "claude-sonnet-4-6", tools: ["cloud_audit", "network_scan"] },
      { role: "Compliance Checker", name: "Mia", model: "claude-sonnet-4-6", tools: ["compliance_check", "report_gen"] },
    ],
    estimatedCostPerRun: 0.65,
    tags: ["security", "audit", "compliance"],
  },
];

// ---------------------------------------------------------------------------
// Page
// ---------------------------------------------------------------------------

export default function TeamsGalleryPage() {
  const params = useParams();
  const router = useRouter();
  const projectId = params.projectId as string;

  const [templates, setTemplates] = useState<TeamTemplate[]>([]);
  const [loading, setLoading] = useState(true);
  const [search, setSearch] = useState("");
  const [category, setCategory] = useState("All");
  const [usingFallback, setUsingFallback] = useState(false);

  useEffect(() => {
    teamApi
      .listTemplates()
      .then((data) => {
        const list = Array.isArray(data) ? data : [];
        if (list.length > 0) {
          setTemplates(list);
          setUsingFallback(false);
        } else {
          setTemplates(FALLBACK_TEMPLATES);
          setUsingFallback(true);
        }
      })
      .catch((err) => {
        console.warn("teams/templates failed — using local fallback list", err);
        setTemplates(FALLBACK_TEMPLATES);
        setUsingFallback(true);
      })
      .finally(() => setLoading(false));
  }, []);

  const filtered = useMemo(() => {
    const q = search.toLowerCase();
    return templates.filter((t) => {
      const name = (t.name ?? "").toLowerCase();
      const desc = (t.description ?? "").toLowerCase();
      const tags = t.tags ?? [];
      const matchesSearch =
        !q ||
        name.includes(q) ||
        desc.includes(q) ||
        tags.some((tag) => (tag ?? "").toLowerCase().includes(q));
      const matchesCategory = category === "All" || t.category === category;
      return matchesSearch && matchesCategory;
    });
  }, [templates, search, category]);

  const handleTemplateClick = (template: TeamTemplate) => {
    router.push(`/${projectId}/teams/new?template=${template.id}`);
  };

  if (loading) {
    return (
      <div className="flex items-center justify-center h-full">
        <Loader2 className="w-6 h-6 animate-spin text-slate-400" />
      </div>
    );
  }

  return (
    <div className="h-full overflow-y-auto scrollbar-thin">
      <div className="max-w-6xl mx-auto px-6 py-6 space-y-6">
        <PageHeader
          title="Teams"
          subtitle="Deploy AI agent teams for complex multi-step tasks"
          actions={
            <Button
              onClick={() => router.push(`/${projectId}/teams/new`)}
              className="bg-gradient-to-r from-glow-cyan to-glow-blue hover:brightness-110 text-white"
            >
              <Plus className="w-4 h-4 mr-2" />
              Create Custom Team
            </Button>
          }
        />

        {usingFallback && (
          <div className="rounded-lg border border-amber-500/20 bg-amber-500/[0.04] p-3 text-xs text-amber-300">
            Showing default team suggestions — live templates aren&apos;t available right now.
          </div>
        )}

        {/* Search and filters */}
        <div className="flex items-center gap-3">
          <div className="relative flex-1 max-w-md">
            <Search className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-slate-400" />
            <input
              type="text"
              value={search}
              onChange={(e) => setSearch(e.target.value)}
              placeholder="Search teams..."
              aria-label="Search teams"
              className="w-full pl-9 pr-3 py-2 text-sm bg-white/[0.03] border border-white/[0.08] rounded-lg text-slate-200 placeholder:text-slate-400/50 focus:outline-none focus:border-white/20"
            />
          </div>
          <div className="flex items-center gap-1.5">
            {CATEGORIES.map((cat) => (
              <button
                key={cat}
                onClick={() => setCategory(cat)}
                className={`px-3 py-1.5 rounded-lg text-xs font-medium transition-colors ${
                  category === cat
                    ? "bg-glow-cyan/[0.08] text-glow-cyan"
                    : "text-slate-400 hover:bg-white/[0.05] hover:text-slate-200"
                }`}
              >
                {cat}
              </button>
            ))}
          </div>
        </div>

        {/* Template grid */}
        {filtered.length === 0 ? (
          <EmptyState
            icon={Users}
            title="No teams found"
            description="Try adjusting your search or filters"
          />
        ) : (
          <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
            {filtered.map((template) => {
              const members = template.members ?? [];
              const tags = template.tags ?? [];
              const cost = template.estimatedCostPerRun ?? 0;
              const proto =
                PROTOCOL_ICONS[template.protocol] ?? PROTOCOL_ICONS.sequential;
              const ProtoIcon = proto.icon;

              return (
                <button
                  key={template.id}
                  onClick={() => handleTemplateClick(template)}
                  className="text-left p-5 rounded-xl border border-white/[0.06] bg-white/[0.02] hover:bg-white/[0.05] hover:border-white/[0.12] transition-all group"
                >
                  {/* Header */}
                  <div className="flex items-start justify-between mb-3">
                    <div className="flex items-center gap-2">
                      <div className="w-9 h-9 rounded-lg bg-glow-cyan/[0.08] flex items-center justify-center">
                        <Users className="w-4.5 h-4.5 text-glow-cyan" />
                      </div>
                      <div>
                        <h3 className="text-sm font-semibold text-slate-200 group-hover:text-glow-cyan transition-colors">
                          {template.name}
                        </h3>
                        <div className="flex items-center gap-1.5 mt-0.5">
                          <ProtoIcon className={`w-3 h-3 ${proto.color}`} />
                          <span className="text-[11px] text-slate-400">{proto.label}</span>
                        </div>
                      </div>
                    </div>
                    <Badge variant="outline" className="text-[10px]">
                      {members.length} members
                    </Badge>
                  </div>

                  {/* Description */}
                  <p className="text-xs text-slate-400 line-clamp-2 mb-3">
                    {template.description}
                  </p>

                  {/* Member avatars */}
                  <div className="flex items-center gap-1 mb-3">
                    {members.slice(0, 5).map((m, i) => (
                      <div
                        key={i}
                        className="w-6 h-6 rounded-full bg-white/[0.06] border border-white/[0.08] flex items-center justify-center"
                        title={`${m.name ?? ""} (${m.role ?? ""})`}
                      >
                        <span className="text-[9px] font-semibold text-slate-400">
                          {m.name?.[0] ?? "?"}
                        </span>
                      </div>
                    ))}
                    {members.length > 5 && (
                      <span className="text-[10px] text-slate-400 ml-1">
                        +{members.length - 5}
                      </span>
                    )}
                  </div>

                  {/* Footer */}
                  <div className="flex items-center justify-between">
                    <div className="flex items-center gap-2">
                      {tags.slice(0, 3).map((tag) => (
                        <span
                          key={tag}
                          className="text-[10px] px-1.5 py-0.5 rounded bg-white/[0.04] text-slate-400/70"
                        >
                          {tag}
                        </span>
                      ))}
                    </div>
                    <span className="text-[11px] text-slate-400">
                      ~${cost.toFixed(2)}/run
                    </span>
                  </div>
                </button>
              );
            })}
          </div>
        )}
      </div>
    </div>
  );
}
