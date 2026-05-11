"use client";

import { useState, useEffect, useCallback } from "react";
import { useParams } from "next/navigation";
import {
  ShieldCheck,
  ShieldAlert,
  CheckCircle2,
  XCircle,
  Clock,
  Loader2,
  RefreshCw,
  AlertTriangle,
  History,
} from "lucide-react";
import { Card, CardContent } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { Tabs, TabsList, TabsTrigger, TabsContent } from "@/components/ui/tabs";
import { useToast } from "@/components/toast";
import { BASE } from "@/lib/api";
import { useVisibleInterval } from "@/hooks/use-visible-interval";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

// Matches backend /projects/:id/gates shape (see hitl_handler.rs).
interface ApprovalRequest {
  id: string;
  gate_type: string;
  status: string;
  description?: string;
  context?: Record<string, unknown>;
  created_at: string;
  resolved_at?: string;
  resolution?: string;
  risk_level?: string;
  agent_name?: string;
}

function gateRisk(req: ApprovalRequest): string {
  if (req.risk_level) return req.risk_level;
  const ctxRisk = (req.context as { risk_level?: string } | undefined)?.risk_level;
  if (ctxRisk) return ctxRisk;
  // Fall back to gate_type hints (e.g. "deploy", "secret" → high).
  const gt = req.gate_type ?? "";
  if (/(deploy|secret|destroy|delete)/i.test(gt)) return "high";
  return "medium";
}

function gateTitle(req: ApprovalRequest): string {
  return req.description || req.gate_type || "Approval requested";
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function riskVariant(level: string) {
  switch (level) {
    case "critical":
      return "destructive" as const;
    case "high":
      return "destructive" as const;
    case "medium":
      return "warning" as const;
    case "low":
      return "secondary" as const;
    default:
      return "secondary" as const;
  }
}

function RiskIcon({ level }: { level: string }) {
  if (level === "critical" || level === "high") {
    return <ShieldAlert className="w-3.5 h-3.5 text-red-400" />;
  }
  if (level === "medium") {
    return <AlertTriangle className="w-3.5 h-3.5 text-amber-400" />;
  }
  return <ShieldCheck className="w-3.5 h-3.5 text-slate-400" />;
}

// ---------------------------------------------------------------------------
// ApprovalCard
// ---------------------------------------------------------------------------

function ApprovalCard({
  request,
  onResolve,
  actionLoading,
}: {
  request: ApprovalRequest;
  onResolve: (id: string, approved: boolean) => void;
  actionLoading: string | null;
}) {
  const isLoading = actionLoading === request.id;
  const isPending = request.status === "pending";
  const risk = gateRisk(request);
  const contextText =
    typeof request.context === "string"
      ? request.context
      : request.context
        ? JSON.stringify(request.context, null, 2)
        : "";

  return (
    <Card className="border-white/[0.08] bg-white/[0.02] hover:bg-white/[0.04] transition-colors">
      <CardContent className="p-4">
        <div className="flex items-start justify-between gap-4">
          <div className="flex items-start gap-3 min-w-0 flex-1">
            <div className="mt-0.5 rounded-lg bg-amber-500/10 p-2 shrink-0">
              <RiskIcon level={risk} />
            </div>
            <div className="min-w-0 flex-1">
              <div className="flex items-center gap-2 mb-1">
                <span className="font-medium text-sm text-slate-200 truncate">
                  {gateTitle(request)}
                </span>
                <Badge variant={riskVariant(risk)} className="text-[10px]">
                  {risk} risk
                </Badge>
                {request.agent_name && (
                  <span className="text-[10px] text-slate-500">{request.agent_name}</span>
                )}
              </div>
              <p className="text-xs text-slate-400 line-clamp-3 mb-2 whitespace-pre-wrap break-words">
                {contextText}
              </p>
              <div className="flex items-center gap-3 text-[10px] text-slate-500">
                <span className="flex items-center gap-1">
                  <Clock className="h-2.5 w-2.5" />
                  {new Date(request.created_at).toLocaleString()}
                </span>
                {request.resolution && (
                  <Badge
                    variant={request.resolution === "approved" ? "success" : "destructive"}
                    className="text-[9px]"
                  >
                    {request.resolution}
                  </Badge>
                )}
              </div>
            </div>
          </div>

          {isPending && (
            <div className="flex items-center gap-1.5 shrink-0">
              <Button
                variant="ghost"
                size="sm"
                className="h-8 text-emerald-400 hover:text-emerald-300 hover:bg-emerald-500/10 text-xs"
                onClick={() => onResolve(request.id, true)}
                disabled={isLoading}
              >
                {isLoading ? (
                  <Loader2 className="h-3.5 w-3.5 animate-spin mr-1" />
                ) : (
                  <CheckCircle2 className="h-3.5 w-3.5 mr-1" />
                )}
                Approve
              </Button>
              <Button
                variant="ghost"
                size="sm"
                className="h-8 text-red-400 hover:text-red-300 hover:bg-red-500/10 text-xs"
                onClick={() => onResolve(request.id, false)}
                disabled={isLoading}
              >
                {isLoading ? (
                  <Loader2 className="h-3.5 w-3.5 animate-spin mr-1" />
                ) : (
                  <XCircle className="h-3.5 w-3.5 mr-1" />
                )}
                Reject
              </Button>
            </div>
          )}
        </div>
      </CardContent>
    </Card>
  );
}

// ---------------------------------------------------------------------------
// Page
// ---------------------------------------------------------------------------

export default function ApprovalsPage() {
  const params = useParams();
  const projectId = params.projectId as string;
  const { toast } = useToast();

  const [pending, setPending] = useState<ApprovalRequest[]>([]);
  const [history, setHistory] = useState<ApprovalRequest[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [actionLoading, setActionLoading] = useState<string | null>(null);

  const fetchApprovals = useCallback(async () => {
    try {
      const res = await fetch(`${BASE}/projects/${projectId}/gates/pending`);
      if (!res.ok) throw new Error("Failed to load approvals");
      const data = await res.json();
      const all: ApprovalRequest[] = Array.isArray(data) ? data : data.gates ?? [];
      setPending(all.filter((a) => a.status === "pending"));
      setHistory(all.filter((a) => a.status !== "pending"));
      setError(null);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to load approvals");
    } finally {
      setLoading(false);
    }
  }, [projectId]);

  useEffect(() => {
    fetchApprovals();
  }, [fetchApprovals]);
  // Pause polling while the tab is hidden to save bandwidth.
  useVisibleInterval(fetchApprovals, 5000, { runOnShow: false });

  const handleResolve = async (id: string, approved: boolean) => {
    setActionLoading(id);
    try {
      const res = await fetch(`${BASE}/projects/${projectId}/gates/resolve`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ gate_id: id, approved }),
      });
      if (!res.ok) throw new Error(await res.text());
      toast("success", approved ? "Gate approved" : "Gate rejected");
      await fetchApprovals();
    } catch (err) {
      const msg = err instanceof Error ? err.message : "Failed to resolve gate";
      toast("error", "Could not update gate", msg);
    } finally {
      setActionLoading(null);
    }
  };

  return (
    <div className="flex flex-col gap-6 p-6 overflow-auto h-full">
      {/* Header */}
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-3">
          <div className="w-9 h-9 rounded-lg bg-gradient-to-br from-amber-500/20 to-orange-500/20 border border-amber-500/10 flex items-center justify-center">
            <ShieldCheck className="w-4 h-4 text-amber-400" />
          </div>
          <div>
            <h1 className="text-lg font-semibold text-slate-200">Approvals</h1>
            <p className="text-xs text-slate-400">
              {loading
                ? "Loading..."
                : `${pending.length} pending approval${pending.length !== 1 ? "s" : ""}`}
            </p>
          </div>
        </div>
        <Button
          variant="ghost"
          size="sm"
          className="text-slate-400 hover:text-slate-200"
          onClick={() => {
            setLoading(true);
            fetchApprovals();
          }}
        >
          <RefreshCw className={`h-4 w-4 mr-1.5 ${loading ? "animate-spin" : ""}`} />
          Refresh
        </Button>
      </div>

      {/* Error */}
      {error && (
        <div className="rounded-lg border border-red-500/20 bg-red-500/5 p-4 text-sm text-red-400">
          {error}
        </div>
      )}

      {/* Tabs */}
      <Tabs defaultValue="pending" className="w-full">
        <TabsList>
          <TabsTrigger value="pending" className="text-xs">
            <ShieldAlert className="h-3.5 w-3.5 mr-1.5" />
            Pending
            {pending.length > 0 && (
              <Badge variant="destructive" className="ml-1.5 text-[9px] px-1.5">
                {pending.length}
              </Badge>
            )}
          </TabsTrigger>
          <TabsTrigger value="history" className="text-xs">
            <History className="h-3.5 w-3.5 mr-1.5" />
            History
          </TabsTrigger>
        </TabsList>

        <TabsContent value="pending" className="mt-4">
          <div className="flex flex-col gap-2">
            {loading ? (
              <>
                {[1, 2, 3].map((i) => (
                  <Card key={i} className="border-white/[0.08] bg-white/[0.02]">
                    <CardContent className="p-4 space-y-2">
                      <Skeleton className="h-4 w-48" />
                      <Skeleton className="h-3 w-full" />
                      <Skeleton className="h-3 w-32" />
                    </CardContent>
                  </Card>
                ))}
              </>
            ) : pending.length === 0 ? (
              <div className="flex flex-col items-center justify-center py-16 text-slate-500">
                <ShieldCheck className="h-10 w-10 mb-3 opacity-40" />
                <p className="text-sm font-medium">No pending approvals</p>
                <p className="text-xs mt-1">All clear — agents are operating within policy</p>
              </div>
            ) : (
              pending.map((req) => (
                <ApprovalCard
                  key={req.id}
                  request={req}
                  onResolve={handleResolve}
                  actionLoading={actionLoading}
                />
              ))
            )}
          </div>
        </TabsContent>

        <TabsContent value="history" className="mt-4">
          <div className="flex flex-col gap-2">
            {history.length === 0 ? (
              <div className="flex flex-col items-center justify-center py-16 text-slate-500">
                <History className="h-10 w-10 mb-3 opacity-40" />
                <p className="text-sm font-medium">No approval history</p>
                <p className="text-xs mt-1">Past decisions will appear here</p>
              </div>
            ) : (
              history.map((req) => (
                <ApprovalCard
                  key={req.id}
                  request={req}
                  onResolve={handleResolve}
                  actionLoading={null}
                />
              ))
            )}
          </div>
        </TabsContent>
      </Tabs>
    </div>
  );
}
