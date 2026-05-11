"use client";

import { useState, useEffect } from "react";
import { useParams, useRouter } from "next/navigation";
import {
  ArrowLeft,
  FileText,
  MessageSquare,
  DollarSign,
  Clock,
  Loader2,
  Copy,
  CheckCircle2,
  ChevronDown,
  ChevronRight,
} from "lucide-react";
import { cn, safeCopyToClipboard } from "@/lib/utils";;
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { PageHeader } from "@/components/page-header";
import { useToast } from "@/components/toast";
import { teamApi, type TeamRunResult, type TeamRunArtifact } from "@/lib/team-api";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function formatDuration(seconds: number): string {
  if (seconds < 60) return `${seconds}s`;
  const m = Math.floor(seconds / 60);
  const s = seconds % 60;
  return s > 0 ? `${m}m ${s}s` : `${m}m`;
}

function formatCost(cost: number): string {
  return cost < 0.01 ? `$${cost.toFixed(4)}` : `$${cost.toFixed(2)}`;
}

// ---------------------------------------------------------------------------
// Page
// ---------------------------------------------------------------------------

export default function TeamResultsPage() {
  const params = useParams();
  const router = useRouter();
  const projectId = params.projectId as string;
  const teamId = params.teamId as string;
  const runId = params.runId as string;
  const { toast } = useToast();

  const [result, setResult] = useState<TeamRunResult | null>(null);
  const [loading, setLoading] = useState(true);
  const [selectedArtifact, setSelectedArtifact] = useState<TeamRunArtifact | null>(null);
  const [showTranscript, setShowTranscript] = useState(false);
  const [copied, setCopied] = useState<string | null>(null);

  useEffect(() => {
    teamApi
      .getRunResult(runId)
      .then(setResult)
      .catch(() => {
        toast("error", "Failed to load results");
      })
      .finally(() => setLoading(false));
  }, [runId, toast]);

  const handleCopy = async (text: string, id: string) => {
    await safeCopyToClipboard(text);
    setCopied(id);
    setTimeout(() => setCopied(null), 2000);
  };

  if (loading) {
    return (
      <div className="flex items-center justify-center h-full">
        <Loader2 className="w-6 h-6 animate-spin text-slate-400" />
      </div>
    );
  }

  if (!result) {
    return (
      <div className="flex flex-col items-center justify-center h-full text-slate-400">
        <p className="text-sm">Results not available yet</p>
        <Button
          variant="ghost"
          className="mt-3"
          onClick={() => router.push(`/${projectId}/teams/${teamId}/runs/${runId}`)}
        >
          <ArrowLeft className="w-4 h-4 mr-2" />
          Back to Execution
        </Button>
      </div>
    );
  }

  return (
    <div className="h-full overflow-y-auto scrollbar-thin">
      <div className="max-w-5xl mx-auto px-6 py-6 space-y-6">
        <PageHeader
          title="Run Results"
          subtitle={result.task}
          actions={
            <Button
              variant="ghost"
              onClick={() => router.push(`/${projectId}/teams/${teamId}/runs/${runId}`)}
              className="text-slate-400"
            >
              <ArrowLeft className="w-4 h-4 mr-2" />
              Back to Execution
            </Button>
          }
        />

        {/* Summary cards */}
        <div className="grid grid-cols-4 gap-3">
          <SummaryCard
            icon={FileText}
            label="Artifacts"
            value={result.artifacts.length.toString()}
            color="text-blue-400"
          />
          <SummaryCard
            icon={MessageSquare}
            label="Messages"
            value={result.messages.length.toString()}
            color="text-purple-400"
          />
          <SummaryCard
            icon={DollarSign}
            label="Total Cost"
            value={formatCost(result.totalCost)}
            color="text-emerald-400"
          />
          <SummaryCard
            icon={Clock}
            label="Duration"
            value={formatDuration(result.duration)}
            color="text-amber-400"
          />
        </div>

        {/* Summary text */}
        {result.summary && (
          <div className="p-4 rounded-xl border border-white/[0.06] bg-white/[0.02]">
            <h3 className="text-[11px] text-slate-400 uppercase tracking-wider mb-2">
              Summary
            </h3>
            <p className="text-sm text-slate-200/80 leading-relaxed">
              {result.summary}
            </p>
          </div>
        )}

        {/* Artifacts */}
        <div>
          <h3 className="text-[11px] text-slate-400 uppercase tracking-wider mb-3">
            Artifacts ({result.artifacts.length})
          </h3>
          {result.artifacts.length === 0 ? (
            <p className="text-sm text-slate-400/50">No artifacts generated</p>
          ) : (
            <div className="space-y-2">
              {result.artifacts.map((artifact) => (
                <div
                  key={artifact.id}
                  className="rounded-xl border border-white/[0.06] bg-white/[0.02] overflow-hidden"
                >
                  <button
                    onClick={() =>
                      setSelectedArtifact(
                        selectedArtifact?.id === artifact.id ? null : artifact
                      )
                    }
                    className="w-full flex items-center gap-3 px-4 py-3 hover:bg-white/[0.02] transition-colors"
                  >
                    <FileText className="w-4 h-4 text-blue-400 flex-shrink-0" />
                    <div className="flex-1 text-left min-w-0">
                      <span className="text-sm font-medium text-slate-200">
                        {artifact.name}
                      </span>
                      <span className="text-xs text-slate-400 ml-2">
                        by {artifact.createdBy}
                      </span>
                    </div>
                    <Badge variant="outline" className="text-[10px]">
                      {artifact.type}
                    </Badge>
                    <span className="text-[10px] text-slate-400">
                      {(artifact.size / 1024).toFixed(1)}KB
                    </span>
                    {selectedArtifact?.id === artifact.id ? (
                      <ChevronDown className="w-3.5 h-3.5 text-slate-400" />
                    ) : (
                      <ChevronRight className="w-3.5 h-3.5 text-slate-400" />
                    )}
                  </button>
                  {selectedArtifact?.id === artifact.id && (
                    <div className="border-t border-white/[0.06] px-4 py-3">
                      <div className="flex items-center justify-end gap-2 mb-2">
                        <button
                          onClick={() => handleCopy(artifact.content, artifact.id)}
                          className="text-[11px] text-slate-400 hover:text-slate-200 transition-colors flex items-center gap-1"
                        >
                          {copied === artifact.id ? (
                            <CheckCircle2 className="w-3 h-3 text-emerald-400" />
                          ) : (
                            <Copy className="w-3 h-3" />
                          )}
                          {copied === artifact.id ? "Copied" : "Copy"}
                        </button>
                      </div>
                      <pre className="text-[12px] text-slate-200/80 bg-white/[0.02] border border-white/[0.06] rounded-lg p-3 overflow-auto max-h-80 scrollbar-thin whitespace-pre-wrap font-mono">
                        {artifact.content}
                      </pre>
                    </div>
                  )}
                </div>
              ))}
            </div>
          )}
        </div>

        {/* Cost breakdown */}
        {result.costBreakdown.length > 0 && (
          <div>
            <h3 className="text-[11px] text-slate-400 uppercase tracking-wider mb-3">
              Cost Breakdown
            </h3>
            <div className="rounded-xl border border-white/[0.06] bg-white/[0.02] overflow-hidden">
              <table className="w-full">
                <thead>
                  <tr className="border-b border-white/[0.06]">
                    <th scope="col" className="text-left text-[11px] text-slate-400 uppercase tracking-wider px-4 py-2">
                      Member
                    </th>
                    <th scope="col" className="text-right text-[11px] text-slate-400 uppercase tracking-wider px-4 py-2">
                      Tokens
                    </th>
                    <th scope="col" className="text-right text-[11px] text-slate-400 uppercase tracking-wider px-4 py-2">
                      Cost
                    </th>
                    <th scope="col" className="text-right text-[11px] text-slate-400 uppercase tracking-wider px-4 py-2">
                      Share
                    </th>
                  </tr>
                </thead>
                <tbody>
                  {result.costBreakdown.map((row) => (
                    <tr key={row.member} className="border-b border-white/[0.04]">
                      <td className="text-sm text-slate-200 px-4 py-2">{row.member}</td>
                      <td className="text-sm text-slate-400 text-right px-4 py-2 tabular-nums">
                        {row.tokens.toLocaleString()}
                      </td>
                      <td className="text-sm text-slate-200 text-right px-4 py-2 tabular-nums">
                        {formatCost(row.cost)}
                      </td>
                      <td className="text-sm text-slate-400 text-right px-4 py-2 tabular-nums">
                        {result.totalCost > 0
                          ? ((row.cost / result.totalCost) * 100).toFixed(0)
                          : 0}
                        %
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </div>
        )}

        {/* Communication transcript */}
        <div>
          <button
            onClick={() => setShowTranscript(!showTranscript)}
            className="flex items-center gap-2 text-[11px] text-slate-400 uppercase tracking-wider mb-3 hover:text-slate-200 transition-colors"
          >
            {showTranscript ? (
              <ChevronDown className="w-3.5 h-3.5" />
            ) : (
              <ChevronRight className="w-3.5 h-3.5" />
            )}
            Communication Transcript ({result.messages.length} messages)
          </button>
          {showTranscript && (
            <div className="rounded-xl border border-white/[0.06] bg-white/[0.02] max-h-96 overflow-y-auto scrollbar-thin">
              {result.messages.map((msg) => (
                <div
                  key={msg.id}
                  className="flex items-start gap-2 px-4 py-2 border-b border-white/[0.04] last:border-0"
                >
                  <span className="text-[10px] text-slate-400/40 tabular-nums flex-shrink-0 mt-0.5">
                    {new Date(msg.timestamp).toLocaleTimeString([], {
                      hour: "2-digit",
                      minute: "2-digit",
                      second: "2-digit",
                    })}
                  </span>
                  <span className="text-xs font-semibold text-blue-400 flex-shrink-0 min-w-[60px]">
                    {msg.from}
                  </span>
                  {msg.to !== "all" && (
                    <span className="text-[10px] text-slate-400/40 flex-shrink-0">
                      {"->"} {msg.to}
                    </span>
                  )}
                  <span className="text-xs text-slate-200/70 flex-1">
                    {msg.content}
                  </span>
                </div>
              ))}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Summary Card
// ---------------------------------------------------------------------------

function SummaryCard({
  icon: Icon,
  label,
  value,
  color,
}: {
  icon: typeof FileText;
  label: string;
  value: string;
  color: string;
}) {
  return (
    <div className="p-4 rounded-xl border border-white/[0.06] bg-white/[0.02]">
      <div className="flex items-center gap-2 mb-2">
        <Icon className={cn("w-4 h-4", color)} />
        <span className="text-[11px] text-slate-400 uppercase tracking-wider">
          {label}
        </span>
      </div>
      <p className="text-xl font-semibold text-slate-200 tabular-nums">{value}</p>
    </div>
  );
}
