"use client";

import { useState, useCallback } from "react";
import { useParams } from "next/navigation";
import {
  Rocket,
  Container,
  Cloud,
  Server,
  Terminal,
  Loader2,
  CheckCircle2,
  XCircle,
  Copy,
  ExternalLink,
  ChevronRight,
} from "lucide-react";
import { cn, safeHttpUrl, safeCopyToClipboard } from "@/lib/utils";;
import { api } from "@/lib/api";
import { Card, CardHeader, CardTitle, CardContent } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";

interface DeployField {
  key: string;
  label: string;
  placeholder: string;
  secret?: boolean;
  required?: boolean;
}

interface DeployTarget {
  id: string;
  name: string;
  description: string;
  icon: React.ElementType;
  color: string;
  bgColor: string;
  available: boolean;
  fields: DeployField[];
}

const TARGETS: DeployTarget[] = [
  {
    id: "docker",
    name: "Docker",
    description: "Build and run as a Docker container locally or on any host",
    icon: Container,
    color: "text-blue-400",
    bgColor: "bg-blue-500/20",
    available: true,
    fields: [
      { key: "port", label: "Port", placeholder: "3000", required: true },
      { key: "tag", label: "Image Tag", placeholder: "latest", required: true },
    ],
  },
  {
    id: "ssh",
    name: "Remote Server (SSH)",
    description: "Deploy via rsync + SSH to any Linux server",
    icon: Server,
    color: "text-emerald-400",
    bgColor: "bg-emerald-500/20",
    available: true,
    fields: [
      { key: "host", label: "Host", placeholder: "user@hostname", required: true },
      { key: "path", label: "Remote Path", placeholder: "/opt/app", required: true },
      { key: "post_deploy", label: "Post-deploy Script", placeholder: "systemctl restart app" },
    ],
  },
  {
    id: "kubernetes",
    name: "Kubernetes",
    description: "Generate K8s manifests and deploy to any cluster",
    icon: Cloud,
    color: "text-purple-400",
    bgColor: "bg-purple-500/20",
    available: true,
    fields: [
      { key: "namespace", label: "Namespace", placeholder: "default", required: true },
      { key: "replicas", label: "Replicas", placeholder: "2", required: true },
      { key: "image", label: "Image", placeholder: "registry/app:latest", required: true },
    ],
  },
  {
    id: "vercel",
    name: "Vercel",
    description: "Deploy Next.js apps to Vercel with zero config",
    icon: Cloud,
    color: "text-white",
    bgColor: "bg-slate-500/20",
    available: true,
    fields: [
      { key: "token", label: "Vercel Token", placeholder: "your_vercel_token", secret: true, required: true },
      { key: "project_name", label: "Project Name", placeholder: "my-app", required: true },
    ],
  },
  {
    id: "railway",
    name: "Railway",
    description: "One-click deploy to Railway's managed infrastructure",
    icon: Terminal,
    color: "text-pink-400",
    bgColor: "bg-pink-500/20",
    available: true,
    fields: [
      { key: "token", label: "Railway Token", placeholder: "your_railway_token", secret: true, required: true },
    ],
  },
];

interface DeployResult {
  status: string;
  url?: string;
  logs?: string;
}

export default function DeployPage() {
  const { projectId } = useParams<{ projectId: string }>();
  const [selected, setSelected] = useState<string | null>(null);
  const [fieldValues, setFieldValues] = useState<Record<string, string>>({});
  const [deploying, setDeploying] = useState(false);
  const [result, setResult] = useState<DeployResult | null>(null);
  const [showLogs, setShowLogs] = useState(false);

  const target = TARGETS.find(t => t.id === selected);

  const missingFields = target
    ? target.fields.filter((f) => f.required && !(fieldValues[f.key] ?? "").trim())
    : [];
  const canDeploy = target != null && missingFields.length === 0;

  const handleDeploy = useCallback(async () => {
    if (!selected || !projectId) return;
    const t = TARGETS.find((x) => x.id === selected);
    if (!t) return;
    const missing = t.fields.filter((f) => f.required && !(fieldValues[f.key] ?? "").trim());
    if (missing.length > 0) {
      setResult({
        status: "error",
        logs: `Missing required fields: ${missing.map((m) => m.label).join(", ")}`,
      });
      return;
    }
    setDeploying(true);
    setResult(null);
    try {
      const res = await api.deployProject(projectId, selected, fieldValues);
      setResult(res);
    } catch (err) {
      setResult({ status: "error", logs: err instanceof Error ? err.message : "Deployment failed" });
    } finally {
      setDeploying(false);
    }
  }, [selected, projectId, fieldValues]);

  return (
    <div className="flex-1 overflow-y-auto">
      <div className="max-w-5xl mx-auto p-6 space-y-6">
        {/* Header */}
        <div>
          <h1 className="text-2xl font-bold text-white flex items-center gap-3">
            <Rocket className="w-7 h-7 text-orange-400" />
            Deploy
          </h1>
          <p className="text-sm text-slate-400 mt-1">
            Ship your project to any infrastructure in one click
          </p>
        </div>

        {/* Target cards */}
        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
          {TARGETS.map(t => (
            <button
              key={t.id}
              onClick={() => {
                setSelected(t.id);
                setFieldValues({});
                setResult(null);
              }}
              className={cn(
                "text-left p-4 rounded-xl border transition-all",
                selected === t.id
                  ? "bg-white/[0.06] border-white/20 ring-1 ring-white/10"
                  : "bg-white/[0.02] border-white/[0.06] hover:bg-white/[0.04]",
                !t.available && "opacity-40 cursor-not-allowed",
              )}
              disabled={!t.available}
            >
              <div className="flex items-center gap-3 mb-2">
                <div className={cn("w-10 h-10 rounded-lg flex items-center justify-center", t.bgColor)}>
                  <t.icon className={cn("w-5 h-5", t.color)} />
                </div>
                <div className="flex-1">
                  <p className="text-sm font-medium text-white">{t.name}</p>
                  {!t.available && <Badge className="text-[9px] bg-slate-500/20 text-slate-400">Coming Soon</Badge>}
                </div>
                {selected === t.id && <CheckCircle2 className="w-5 h-5 text-blue-400" />}
              </div>
              <p className="text-xs text-slate-400">{t.description}</p>
            </button>
          ))}
        </div>

        {/* Configuration */}
        {target && (
          <Card className="bg-white/[0.03] border-white/[0.06]">
            <CardHeader>
              <CardTitle className="text-base text-white flex items-center gap-2">
                <target.icon className={cn("w-5 h-5", target.color)} />
                Configure {target.name} Deployment
              </CardTitle>
            </CardHeader>
            <CardContent className="space-y-4">
              {target.fields.map(f => (
                <div key={f.key}>
                  <label className="text-xs text-slate-400 block mb-1">
                    {f.label}
                    {f.required && <span className="text-red-400 ml-1">*</span>}
                  </label>
                  <input
                    type={f.secret ? "password" : "text"}
                    required={f.required}
                    aria-required={f.required}
                    className="w-full bg-white/[0.05] border border-white/10 rounded-lg px-3 py-2 text-sm text-white placeholder-slate-500 focus:outline-none focus:ring-1 focus:ring-blue-500/50"
                    placeholder={f.placeholder}
                    value={fieldValues[f.key] ?? ""}
                    onChange={e => setFieldValues(v => ({ ...v, [f.key]: e.target.value }))}
                  />
                </div>
              ))}
              <div className="flex items-center gap-3 pt-2">
                <Button onClick={handleDeploy} disabled={deploying || !canDeploy}>
                  {deploying ? (
                    <Loader2 className="w-4 h-4 mr-2 animate-spin" />
                  ) : (
                    <Rocket className="w-4 h-4 mr-2" />
                  )}
                  Deploy to {target.name}
                </Button>
                {missingFields.length > 0 && (
                  <span className="text-xs text-amber-400">
                    Fill: {missingFields.map((m) => m.label).join(", ")}
                  </span>
                )}
              </div>
            </CardContent>
          </Card>
        )}

        {/* Result */}
        {result && (
          <Card className={cn(
            "border",
            result.status === "error" ? "bg-red-500/5 border-red-500/20" : "bg-emerald-500/5 border-emerald-500/20",
          )}>
            <CardContent className="p-4 space-y-3">
              <div className="flex items-center gap-3">
                {result.status === "error" ? (
                  <XCircle className="w-6 h-6 text-red-400" />
                ) : (
                  <CheckCircle2 className="w-6 h-6 text-emerald-400" />
                )}
                <div className="flex-1">
                  <p className={cn("font-medium", result.status === "error" ? "text-red-300" : "text-emerald-300")}>
                    {result.status === "error" ? "Deployment Failed" : "Deployment Successful"}
                  </p>
                  {result.url && (
                    <div className="flex items-center gap-2 mt-1">
                      <code className="text-xs text-slate-300 font-mono">{result.url}</code>
                      <button onClick={() => void safeCopyToClipboard(result.url!)} className="text-slate-400 hover:text-white">
                        <Copy className="w-3.5 h-3.5" />
                      </button>
                      {safeHttpUrl(result.url) && (
                        <a
                          href={safeHttpUrl(result.url) ?? "#"}
                          target="_blank"
                          rel="noopener noreferrer"
                          className="text-slate-400 hover:text-white"
                        >
                          <ExternalLink className="w-3.5 h-3.5" />
                        </a>
                      )}
                    </div>
                  )}
                </div>
              </div>
              {result.logs && (
                <>
                  <button
                    onClick={() => setShowLogs(!showLogs)}
                    className="flex items-center gap-1 text-xs text-slate-400 hover:text-white"
                  >
                    <ChevronRight className={cn("w-3 h-3 transition-transform", showLogs && "rotate-90")} />
                    {showLogs ? "Hide" : "Show"} logs
                  </button>
                  {showLogs && (
                    <pre className="bg-black/30 rounded-lg p-3 text-xs text-slate-300 font-mono whitespace-pre-wrap overflow-auto max-h-[300px]">
                      {result.logs}
                    </pre>
                  )}
                </>
              )}
            </CardContent>
          </Card>
        )}
      </div>
    </div>
  );
}
