"use client";

import { useState, useEffect, useCallback } from "react";
import { useParams } from "next/navigation";
import {
  Scale,
  ShieldAlert,
  ShieldCheck,
  Plus,
  Trash2,
  Power,
  PowerOff,
  AlertTriangle,
  CheckCircle2,
  XCircle,
  ScanEye,
  Loader2,
  ChevronRight,
  ToggleLeft,
  ToggleRight,
  FileWarning,
} from "lucide-react";
import { cn } from "@/lib/utils";
import { api, type GovernancePolicy, type ComplianceReport, type BackendPolicyRule } from "@/lib/api";
import { Card, CardHeader, CardTitle, CardContent } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { useToast } from "@/components/toast";

interface PolicyFormState {
  id: string;
  name: string;
  description: string;
  enabled: boolean;
  backendRules: BackendPolicyRule[];
  legacyField: string;
  legacyOperator: string;
  legacyValue: string;
}

const EMPTY_POLICY: PolicyFormState = {
  id: "",
  name: "",
  description: "",
  enabled: true,
  backendRules: [],
  legacyField: "tool_name",
  legacyOperator: "equals",
  legacyValue: "",
};

const POLICY_TEMPLATES: { name: string; description: string; rules: BackendPolicyRule[] }[] = [
  {
    name: "Block Dangerous Tools",
    description: "Prevent agents from using destructive file-system operations",
    rules: [{ type: "forbidden_tools", tools: ["rm_rf", "format_disk", "drop_database"] }],
  },
  {
    name: "Cost Guard ($10 limit)",
    description: "Deny any single action costing more than $10",
    rules: [{ type: "max_cost_usd", limit: 10 }],
  },
  {
    name: "Require Approval for Deploy",
    description: "Human approval needed before any deployment action",
    rules: [{ type: "require_approval", tags: ["deploy"] }],
  },
  {
    name: "Block PII Exposure",
    description: "Prevent agents from sending data flagged with PII",
    rules: [{ type: "block_pii" }],
  },
];

export default function GovernancePage() {
  const { projectId: _projectId } = useParams<{ projectId: string }>();
  const { toast } = useToast();
  const [policies, setPolicies] = useState<GovernancePolicy[]>([]);
  const [halted, setHalted] = useState(false);
  const [loading, setLoading] = useState(true);
  const [showForm, setShowForm] = useState(false);
  const [form, setForm] = useState<PolicyFormState>(EMPTY_POLICY);
  const [saving, setSaving] = useState(false);
  const [killSwitchLoading, setKillSwitchLoading] = useState(false);

  const [complianceAction, setComplianceAction] = useState("");
  const [complianceReport, setComplianceReport] = useState<ComplianceReport | null>(null);
  const [complianceLoading, setComplianceLoading] = useState(false);

  const [piiText, setPiiText] = useState("");
  const [piiResult, setPiiResult] = useState<{ redacted: string; detected_pii_types: string[]; pii_found: boolean } | null>(null);
  const [piiLoading, setPiiLoading] = useState(false);

  const fetchPolicies = useCallback(async () => {
    try {
      const data = await api.listPolicies();
      setPolicies(data.policies ?? []);
      setHalted(data.halted ?? false);
    } catch {
      // empty
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    fetchPolicies();
  }, [fetchPolicies]);

  async function handleSavePolicy() {
    if (saving) return;
    const id = form.id || `policy_${Date.now()}`;
    const rules: BackendPolicyRule[] = form.backendRules.length > 0
      ? form.backendRules
      : buildRuleFromLegacy(form.legacyField, form.legacyOperator, form.legacyValue);
    const payload = {
      id,
      name: form.name,
      applies_to: ["*"] as string[],
      rules,
      enabled: form.enabled,
    };
    setSaving(true);
    try {
      await api.upsertPolicy(payload);
    } catch (err) {
      console.error("Failed to save governance policy", err);
      setSaving(false);
      toast("error", "Save failed", err instanceof Error ? err.message : "Could not save policy");
      return;
    }
    // Close form and reset BEFORE refreshing policies list
    setSaving(false);
    setShowForm(false);
    setForm(EMPTY_POLICY);
    toast("success", "Policy saved", `"${payload.name}" is now active`);
    // Refresh in the background
    fetchPolicies().catch(console.error);
  }

  function buildRuleFromLegacy(field: string, operator: string, value: string): BackendPolicyRule[] {
    if (field === "tool_name" && (operator === "in" || operator === "equals")) {
      return [{ type: "forbidden_tools", tools: value.split(",").map((s) => s.trim()) }];
    }
    if (field === "estimated_cost_usd" && operator === "greater_than") {
      return [{ type: "max_cost_usd", limit: parseFloat(value) || 10 }];
    }
    if (field === "action_tags" && value.includes("deploy")) {
      return [{ type: "require_approval", tags: value.split(",").map((s) => s.trim()) }];
    }
    if (field === "action_tags" && value.includes("pii")) {
      return [{ type: "block_pii" }];
    }
    return [{ type: "forbidden_tools", tools: value.split(",").map((s) => s.trim()) }];
  }

  async function handleDeletePolicy(id: string) {
    try {
      await api.deletePolicy(id);
      await fetchPolicies();
      toast("success", "Policy deleted");
    } catch (err) {
      console.error("Failed to delete governance policy", err);
      toast("error", "Delete failed", err instanceof Error ? err.message : "Could not delete policy");
    }
  }

  async function handleKillSwitch() {
    setKillSwitchLoading(true);
    try {
      const res = await api.killSwitch(!halted, halted ? "Deactivated by operator" : "Activated by operator");
      setHalted(res.halted);
      toast("success", res.halted ? "Kill switch activated" : "Kill switch deactivated");
    } catch (err) {
      toast("error", "Kill switch failed", err instanceof Error ? err.message : "Request failed");
    } finally {
      setKillSwitchLoading(false);
    }
  }

  async function handleComplianceCheck() {
    if (!complianceAction.trim()) return;
    setComplianceLoading(true);
    try {
      const report = await api.checkCompliance({ action: complianceAction });
      setComplianceReport(report);
    } catch (err) {
      toast("error", "Compliance check failed", err instanceof Error ? err.message : "Request failed");
    } finally {
      setComplianceLoading(false);
    }
  }

  async function handlePiiRedact() {
    if (!piiText.trim()) return;
    setPiiLoading(true);
    try {
      const result = await api.redactPii(piiText);
      setPiiResult(result);
    } catch (err) {
      toast("error", "PII redaction failed", err instanceof Error ? err.message : "Request failed");
    } finally {
      setPiiLoading(false);
    }
  }

  function applyTemplate(tpl: (typeof POLICY_TEMPLATES)[number]) {
    setForm({
      ...EMPTY_POLICY,
      name: tpl.name,
      description: tpl.description,
      backendRules: tpl.rules,
    });
    setShowForm(true);
  }

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
              <Scale className="w-7 h-7 text-amber-400" />
              Governance
            </h1>
            <p className="text-sm text-slate-400 mt-1">
              Policies, compliance, kill-switch, and PII redaction
            </p>
          </div>
          <div className="flex items-center gap-3">
            <Button
              variant="outline"
              size="sm"
              onClick={() => { setForm(EMPTY_POLICY); setShowForm(true); }}
              className="border-white/10 hover:bg-white/5"
            >
              <Plus className="w-4 h-4 mr-1" /> New Policy
            </Button>
            <Button
              variant={halted ? "default" : "destructive"}
              size="sm"
              onClick={handleKillSwitch}
              disabled={killSwitchLoading}
            >
              <span className="inline-flex items-center gap-1">
                {killSwitchLoading ? (
                  <Loader2 className="w-4 h-4 animate-spin" />
                ) : halted ? (
                  <Power className="w-4 h-4" />
                ) : (
                  <PowerOff className="w-4 h-4" />
                )}
                {halted ? "Resume Agents" : "Kill Switch"}
              </span>
            </Button>
          </div>
        </div>

        {/* Kill-switch banner */}
        {halted && (
          <div className="bg-red-500/10 border border-red-500/30 rounded-xl p-4 flex items-center gap-3">
            <ShieldAlert className="w-6 h-6 text-red-400 flex-shrink-0" />
            <div>
              <p className="text-red-300 font-medium">Kill Switch Active</p>
              <p className="text-red-400/70 text-sm">
                All agent execution is halted globally. Click &quot;Resume Agents&quot; to restore operations.
              </p>
            </div>
          </div>
        )}

        {/* Stats row */}
        <div className="grid grid-cols-1 md:grid-cols-4 gap-4">
          <Card className="bg-white/[0.03] border-white/[0.06]">
            <CardContent className="p-4 flex items-center gap-3">
              <div className={cn("w-10 h-10 rounded-lg flex items-center justify-center", halted ? "bg-red-500/20" : "bg-emerald-500/20")}>
                {halted ? <ShieldAlert className="w-5 h-5 text-red-400" /> : <ShieldCheck className="w-5 h-5 text-emerald-400" />}
              </div>
              <div>
                <p className="text-xs text-slate-500">System Status</p>
                <p className={cn("text-lg font-semibold", halted ? "text-red-400" : "text-emerald-400")}>
                  {halted ? "HALTED" : "RUNNING"}
                </p>
              </div>
            </CardContent>
          </Card>
          <Card className="bg-white/[0.03] border-white/[0.06]">
            <CardContent className="p-4 flex items-center gap-3">
              <div className="w-10 h-10 rounded-lg bg-blue-500/20 flex items-center justify-center">
                <Scale className="w-5 h-5 text-blue-400" />
              </div>
              <div>
                <p className="text-xs text-slate-500">Active Policies</p>
                <p className="text-lg font-semibold text-white">{policies.filter(p => p.enabled).length}</p>
              </div>
            </CardContent>
          </Card>
          <Card className="bg-white/[0.03] border-white/[0.06]">
            <CardContent className="p-4 flex items-center gap-3">
              <div className="w-10 h-10 rounded-lg bg-amber-500/20 flex items-center justify-center">
                <AlertTriangle className="w-5 h-5 text-amber-400" />
              </div>
              <div>
                <p className="text-xs text-slate-500">Total Policies</p>
                <p className="text-lg font-semibold text-white">{policies.length}</p>
              </div>
            </CardContent>
          </Card>
          <Card className="bg-white/[0.03] border-white/[0.06]">
            <CardContent className="p-4 flex items-center gap-3">
              <div className="w-10 h-10 rounded-lg bg-purple-500/20 flex items-center justify-center">
                <ScanEye className="w-5 h-5 text-purple-400" />
              </div>
              <div>
                <p className="text-xs text-slate-500">PII Detection</p>
                <p className="text-lg font-semibold text-white">Active</p>
              </div>
            </CardContent>
          </Card>
        </div>

        {/* Policy form */}
        {showForm && (
          <Card key={form.id || "new-policy"} className="bg-white/[0.03] border-white/[0.06]">
            <CardHeader>
              <CardTitle className="text-lg text-white">
                {form.id ? "Edit Policy" : "Create Policy"}
              </CardTitle>
            </CardHeader>
            <CardContent className="space-y-4">
              <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
                <div>
                  <label className="text-xs text-slate-400 block mb-1">Name</label>
                  <input
                    className="w-full bg-white/[0.05] border border-white/10 rounded-lg px-3 py-2 text-sm text-white placeholder-slate-500 focus:outline-none focus:ring-1 focus:ring-blue-500/50"
                    placeholder="e.g. Block destructive tools"
                    value={form.name}
                    onChange={e => setForm(f => ({ ...f, name: e.target.value }))}
                  />
                </div>
                <div>
                  <label className="text-xs text-slate-400 block mb-1">Description</label>
                  <input
                    className="w-full bg-white/[0.05] border border-white/10 rounded-lg px-3 py-2 text-sm text-white placeholder-slate-500 focus:outline-none focus:ring-1 focus:ring-blue-500/50"
                    placeholder="What this policy enforces"
                    value={form.description}
                    onChange={e => setForm(f => ({ ...f, description: e.target.value }))}
                  />
                </div>
              </div>

              <div>
                <label className="text-xs text-slate-400 block mb-2">Rules</label>
                {form.backendRules.length > 0 ? (
                  <div className="space-y-2">
                    {form.backendRules.map((rule, i) => (
                      <div key={i} className="flex items-center gap-2 p-2 rounded-lg bg-white/[0.03] border border-white/[0.06]">
                        <Badge variant="outline" className="text-xs">{rule.type}</Badge>
                        <span className="text-xs text-slate-400">
                          {"tools" in rule ? (rule as { tools: string[] }).tools.join(", ") : ""}
                          {"limit" in rule ? `$${(rule as { limit: number }).limit}` : ""}
                          {"tags" in rule ? (rule as { tags: string[] }).tags.join(", ") : ""}
                        </span>
                      </div>
                    ))}
                  </div>
                ) : (
                  <div className="flex items-center gap-2 mb-2">
                    <select
                      className="bg-white/[0.05] border border-white/10 rounded-lg px-3 py-2 text-sm text-white"
                      value={form.legacyField}
                      onChange={e => setForm(f => ({ ...f, legacyField: e.target.value }))}
                    >
                      <option value="tool_name">Tool Name</option>
                      <option value="agent_id">Agent ID</option>
                      <option value="estimated_cost_usd">Est. Cost (USD)</option>
                      <option value="action_tags">Action Tags</option>
                    </select>
                    <select
                      className="bg-white/[0.05] border border-white/10 rounded-lg px-3 py-2 text-sm text-white"
                      value={form.legacyOperator}
                      onChange={e => setForm(f => ({ ...f, legacyOperator: e.target.value }))}
                    >
                      <option value="equals">equals</option>
                      <option value="not_equals">not equals</option>
                      <option value="contains">contains</option>
                      <option value="in">in (comma-separated)</option>
                      <option value="greater_than">greater than</option>
                      <option value="less_than">less than</option>
                    </select>
                    <input
                      className="flex-1 bg-white/[0.05] border border-white/10 rounded-lg px-3 py-2 text-sm text-white placeholder-slate-500 focus:outline-none focus:ring-1 focus:ring-blue-500/50"
                      placeholder="Value"
                      value={form.legacyValue}
                      onChange={e => setForm(f => ({ ...f, legacyValue: e.target.value }))}
                    />
                  </div>
                )}
                <Button
                  variant="ghost"
                  size="sm"
                  onClick={() => setForm(f => ({ ...f, backendRules: [...f.backendRules, { type: "forbidden_tools", tools: [] }] }))}
                  className="text-slate-400"
                >
                  <Plus className="w-3 h-3 mr-1" /> Add Rule
                </Button>
              </div>

              <div className="flex items-center justify-between pt-2">
                <label className="flex items-center gap-2 text-sm text-slate-300 cursor-pointer">
                  <button onClick={() => setForm(f => ({ ...f, enabled: !f.enabled }))}>
                    {form.enabled ? (
                      <ToggleRight className="w-6 h-6 text-emerald-400" />
                    ) : (
                      <ToggleLeft className="w-6 h-6 text-slate-500" />
                    )}
                  </button>
                  {form.enabled ? "Enabled" : "Disabled"}
                </label>
                <div className="flex gap-2">
                  <Button variant="ghost" size="sm" onClick={() => setShowForm(false)}>Cancel</Button>
                  <Button size="sm" onClick={handleSavePolicy} disabled={saving || !form.name}>
                    {saving ? "Saving…" : "Save Policy"}
                  </Button>
                </div>
              </div>
            </CardContent>
          </Card>
        )}

        {/* Policy templates */}
        {!showForm && policies.length === 0 && (
          <Card className="bg-white/[0.03] border-white/[0.06]">
            <CardHeader>
              <CardTitle className="text-base text-white">Quick Start Templates</CardTitle>
            </CardHeader>
            <CardContent className="grid grid-cols-1 md:grid-cols-2 gap-3">
              {POLICY_TEMPLATES.map((tpl) => (
                <button
                  key={tpl.name}
                  onClick={() => applyTemplate(tpl)}
                  className="text-left p-3 rounded-lg bg-white/[0.03] border border-white/[0.06] hover:bg-white/[0.06] transition-colors"
                >
                  <p className="text-sm font-medium text-white">{tpl.name}</p>
                  <p className="text-xs text-slate-400 mt-1">{tpl.description}</p>
                </button>
              ))}
            </CardContent>
          </Card>
        )}

        {/* Policies list */}
        {policies.length > 0 && (
          <Card className="bg-white/[0.03] border-white/[0.06]">
            <CardHeader>
              <CardTitle className="text-base text-white">Active Policies</CardTitle>
            </CardHeader>
            <CardContent className="space-y-2">
              {policies.map((p) => (
                <div
                  key={p.id}
                  className="flex items-center justify-between p-3 rounded-lg bg-white/[0.02] border border-white/[0.04] hover:bg-white/[0.04] transition-colors"
                >
                  <div className="flex items-center gap-3">
                    {p.enabled ? (
                      <CheckCircle2 className="w-5 h-5 text-emerald-400" />
                    ) : (
                      <XCircle className="w-5 h-5 text-slate-500" />
                    )}
                    <div>
                      <p className="text-sm font-medium text-white">{p.name || p.id}</p>
                      {p.description && (
                        <p className="text-xs text-slate-400">{p.description}</p>
                      )}
                    </div>
                  </div>
                  <div className="flex items-center gap-2">
                    <Badge variant={p.enabled ? "default" : "secondary"} className="text-[10px]">
                      {p.rules?.length ?? 0} rule{(p.rules?.length ?? 0) !== 1 ? "s" : ""}
                    </Badge>
                    <button
                      onClick={() => {
                        setForm({ id: p.id, name: p.name, description: "", enabled: p.enabled, backendRules: p.rules ?? [], legacyField: "tool_name", legacyOperator: "equals", legacyValue: "" });
                        setShowForm(true);
                      }}
                      className="text-slate-400 hover:text-white p-1"
                    >
                      <ChevronRight className="w-4 h-4" />
                    </button>
                    <button
                      onClick={() => handleDeletePolicy(p.id)}
                      className="text-slate-500 hover:text-red-400 p-1"
                    >
                      <Trash2 className="w-4 h-4" />
                    </button>
                  </div>
                </div>
              ))}
            </CardContent>
          </Card>
        )}

        {/* Compliance Checker + PII Redactor side by side */}
        <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
          {/* Compliance Checker */}
          <Card className="bg-white/[0.03] border-white/[0.06]">
            <CardHeader>
              <CardTitle className="text-base text-white flex items-center gap-2">
                <ShieldCheck className="w-5 h-5 text-blue-400" />
                Compliance Checker
              </CardTitle>
            </CardHeader>
            <CardContent className="space-y-3">
              <input
                className="w-full bg-white/[0.05] border border-white/10 rounded-lg px-3 py-2 text-sm text-white placeholder-slate-500 focus:outline-none focus:ring-1 focus:ring-blue-500/50"
                placeholder="Describe an action to check, e.g. deploy_to_production"
                value={complianceAction}
                onChange={e => setComplianceAction(e.target.value)}
                onKeyDown={e => e.key === "Enter" && handleComplianceCheck()}
              />
              <Button size="sm" onClick={handleComplianceCheck} disabled={complianceLoading || !complianceAction.trim()}>
                <span className="inline-flex items-center gap-1">
                  {complianceLoading ? <Loader2 className="w-4 h-4 animate-spin" /> : <ShieldCheck className="w-4 h-4" />}
                  Check Compliance
                </span>
              </Button>
              {complianceReport && (
                <div className="mt-3 space-y-2">
                  <div className="flex items-center gap-2">
                    <Badge className={cn(
                      "text-xs",
                      complianceReport.score >= 80 ? "bg-emerald-500/20 text-emerald-300" :
                      complianceReport.score >= 50 ? "bg-amber-500/20 text-amber-300" :
                      "bg-red-500/20 text-red-300"
                    )}>
                      Grade: {complianceReport.grade} ({complianceReport.score}/100)
                    </Badge>
                  </div>
                  {complianceReport.checks?.map((c, i) => (
                    <div key={i} className="flex items-center gap-2 text-xs">
                      {c.passed ? (
                        <CheckCircle2 className="w-3.5 h-3.5 text-emerald-400" />
                      ) : (
                        <XCircle className="w-3.5 h-3.5 text-red-400" />
                      )}
                      <span className={c.passed ? "text-slate-300" : "text-red-300"}>
                        {c.name}
                      </span>
                      {c.detail && <span className="text-slate-500">- {c.detail}</span>}
                    </div>
                  ))}
                </div>
              )}
            </CardContent>
          </Card>

          {/* PII Redactor */}
          <Card className="bg-white/[0.03] border-white/[0.06]">
            <CardHeader>
              <CardTitle className="text-base text-white flex items-center gap-2">
                <FileWarning className="w-5 h-5 text-amber-400" />
                PII Redactor
              </CardTitle>
            </CardHeader>
            <CardContent className="space-y-3">
              <textarea
                className="w-full bg-white/[0.05] border border-white/10 rounded-lg px-3 py-2 text-sm text-white placeholder-slate-500 focus:outline-none focus:ring-1 focus:ring-blue-500/50 min-h-[80px] resize-none"
                placeholder="Paste text to scan for PII (emails, phones, SSNs, etc.)"
                value={piiText}
                onChange={e => setPiiText(e.target.value)}
                rows={3}
              />
              <Button size="sm" onClick={handlePiiRedact} disabled={piiLoading || !piiText.trim()}>
                <span className="inline-flex items-center gap-1">
                  {piiLoading ? <Loader2 className="w-4 h-4 animate-spin" /> : <ScanEye className="w-4 h-4" />}
                  Scan & Redact
                </span>
              </Button>
              {piiResult && (
                <div className="mt-3 space-y-2">
                  {piiResult.pii_found ? (
                    <>
                      <Badge className="bg-red-500/20 text-red-300 text-xs">
                        PII Detected: {piiResult.detected_pii_types.join(", ")}
                      </Badge>
                      <div className="bg-white/[0.03] rounded-lg p-3 text-sm text-slate-300 font-mono whitespace-pre-wrap">
                        {piiResult.redacted}
                      </div>
                    </>
                  ) : (
                    <Badge className="bg-emerald-500/20 text-emerald-300 text-xs">
                      No PII detected
                    </Badge>
                  )}
                </div>
              )}
            </CardContent>
          </Card>
        </div>
      </div>
    </div>
  );
}
