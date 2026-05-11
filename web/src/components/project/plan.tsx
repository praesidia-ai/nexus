"use client";

import { useState } from "react";
import {
  useQuery,
  useMutation,
  useQueryClient,
} from "@tanstack/react-query";
import {
  CheckCircle2,
  Circle,
  Loader2,
  FileCode2,
  ListChecks,
  RefreshCw,
  Rocket,
  AlertTriangle,
  Sparkles,
} from "lucide-react";
import { cn } from "@/lib/utils";
import { api } from "@/lib/api";
import { Skeleton } from "@/components/ui/skeleton";
import { GlowButton } from "@/components/ui/glow-button";
import { GlowCard } from "@/components/ui/glow-card";
import { Badge } from "@/components/ui/badge";
import { StaggerItem } from "@/components/ui/stagger-list";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface PlanStep {
  name: string;
  description: string;
  status?: string;
}

interface Plan {
  steps: PlanStep[];
  estimated_files: number;
  [key: string]: unknown;
}

interface ProjectPlanProps {
  projectId: string;
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function stepIcon(status?: string) {
  switch (status) {
    case "done":
    case "completed":
      return <CheckCircle2 className="h-5 w-5 text-emerald-400 shrink-0" />;
    case "in-progress":
    case "running":
      return <Loader2 className="h-5 w-5 text-glow-cyan animate-spin shrink-0" />;
    default:
      return <Circle className="h-5 w-5 text-slate-500 shrink-0" />;
  }
}

// ---------------------------------------------------------------------------
// Loading skeleton
// ---------------------------------------------------------------------------

function PlanSkeleton() {
  return (
    <div className="space-y-4 p-6">
      <Skeleton className="h-6 w-48" />
      <Skeleton className="h-4 w-32" />
      <div className="space-y-3 mt-6">
        {Array.from({ length: 4 }).map((_, i) => (
          <div key={i} className="flex items-start gap-3">
            <Skeleton className="h-5 w-5 rounded-full shrink-0 mt-0.5" />
            <div className="flex-1 space-y-2">
              <Skeleton className="h-4 w-40" />
              <Skeleton className="h-3 w-64" />
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export function ProjectPlan({ projectId }: ProjectPlanProps) {
  const queryClient = useQueryClient();
  const [description, setDescription] = useState("");

  // ---- Fetch current plan ----
  const {
    data,
    isLoading,
    isError,
    error,
    refetch,
  } = useQuery({
    queryKey: ["plan", projectId],
    queryFn: () => api.getPlan(projectId),
  });

  // ---- Generate plan mutation ----
  const generateMutation = useMutation({
    mutationFn: (desc: string) => api.generatePlan(projectId, desc),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["plan", projectId] });
    },
  });

  // ---- Approve plan mutation ----
  const approveMutation = useMutation({
    mutationFn: (plan: unknown) => api.approvePlan(projectId, plan),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["plan", projectId] });
    },
  });

  // ---- Loading state ----
  if (isLoading) {
    return (
      <div className="flex-1 overflow-auto p-6">
        <GlowCard className="p-0 overflow-hidden">
          <PlanSkeleton />
        </GlowCard>
      </div>
    );
  }

  // ---- Error state ----
  if (isError) {
    return (
      <div className="flex-1 flex items-center justify-center p-6">
        <GlowCard className="max-w-md w-full p-8 text-center">
          <AlertTriangle className="h-10 w-10 text-red-400 mx-auto" />
          <h3 className="mt-4 text-lg font-semibold text-slate-200">
            Failed to load plan
          </h3>
          <p className="mt-1 text-sm text-slate-400">
            {error instanceof Error ? error.message : "An unexpected error occurred."}
          </p>
          <GlowButton
            variant="secondary"
            size="sm"
            className="mt-4"
            onClick={() => refetch()}
          >
            <RefreshCw className="h-4 w-4" />
            Retry
          </GlowButton>
        </GlowCard>
      </div>
    );
  }

  const plan = data?.plan as Plan | undefined;
  const planExists = data?.exists && plan;

  // ---- Empty state: no plan yet ----
  if (!planExists) {
    return (
      <div className="flex-1 flex items-center justify-center p-6">
        <GlowCard className="max-w-lg w-full p-8">
          <div className="flex flex-col items-center text-center">
            <ListChecks className="h-12 w-12 text-slate-400/30" />
            <h3 className="mt-4 text-lg font-semibold text-slate-200">
              No plan yet
            </h3>
            <p className="mt-1 text-sm text-slate-400">
              Describe what you want to build and we will generate a step-by-step plan.
            </p>
          </div>

          <textarea
            className={cn(
              "mt-6 w-full rounded-lg border border-white/[0.06] bg-white/[0.03] p-3",
              "text-sm text-slate-200 placeholder:text-slate-500",
              "focus:outline-none focus:ring-2 focus:ring-glow-cyan/40 focus:border-glow-cyan/20",
              "resize-none min-h-[120px]",
            )}
            placeholder="Describe the features, pages, or functionality you need..."
            value={description}
            onChange={(e) => setDescription(e.target.value)}
            disabled={generateMutation.isPending}
          />

          <GlowButton
            className="mt-4 w-full"
            disabled={!description.trim() || generateMutation.isPending}
            onClick={() => generateMutation.mutate(description.trim())}
          >
            {generateMutation.isPending ? (
              <Loader2 className="h-4 w-4 animate-spin" />
            ) : (
              <Sparkles className="h-4 w-4" />
            )}
            {generateMutation.isPending ? "Generating..." : "Generate Plan"}
          </GlowButton>

          {generateMutation.isError && (
            <p className="mt-3 text-xs text-red-400 text-center">
              {generateMutation.error instanceof Error
                ? generateMutation.error.message
                : "Failed to generate plan. Please try again."}
            </p>
          )}
        </GlowCard>
      </div>
    );
  }

  // ---- Plan exists: show timeline ----
  const steps: PlanStep[] = plan.steps ?? [];
  const estimatedFiles = plan.estimated_files ?? 0;
  const completedSteps = steps.filter(
    (s) => s.status === "done" || s.status === "completed",
  ).length;

  return (
    <div className="flex-1 overflow-auto p-6 space-y-4">
      {/* Header */}
      <div className="flex items-center justify-between">
        <div>
          <h2 className="text-lg font-semibold text-slate-200 flex items-center gap-2">
            <ListChecks className="h-5 w-5 text-glow-cyan" />
            Execution Plan
          </h2>
          <div className="mt-1 flex items-center gap-3 text-sm text-slate-400">
            <span>
              {completedSteps}/{steps.length} steps
            </span>
            <span className="text-slate-600">|</span>
            <span className="flex items-center gap-1">
              <FileCode2 className="h-3.5 w-3.5" />
              ~{estimatedFiles} files estimated
            </span>
          </div>
        </div>

        <div className="flex items-center gap-2">
          <GlowButton
            variant="ghost"
            size="sm"
            disabled={generateMutation.isPending}
            onClick={() => {
              if (description.trim()) {
                generateMutation.mutate(description.trim());
              } else {
                // Re-generate with same context
                generateMutation.mutate("Regenerate the current plan with improvements");
              }
            }}
          >
            {generateMutation.isPending ? (
              <Loader2 className="h-4 w-4 animate-spin" />
            ) : (
              <RefreshCw className="h-4 w-4" />
            )}
            Regenerate
          </GlowButton>

          <GlowButton
            size="sm"
            disabled={approveMutation.isPending}
            onClick={() => approveMutation.mutate(plan)}
          >
            {approveMutation.isPending ? (
              <Loader2 className="h-4 w-4 animate-spin" />
            ) : (
              <Rocket className="h-4 w-4" />
            )}
            {approveMutation.isPending ? "Executing..." : "Approve & Execute"}
          </GlowButton>
        </div>
      </div>

      {/* Approve/execute result feedback */}
      {approveMutation.isSuccess && (
        <GlowCard glow="cyan" active className="p-4">
          <div className="flex items-center gap-2 text-sm text-emerald-400">
            <CheckCircle2 className="h-4 w-4" />
            Plan approved and execution started successfully.
          </div>
        </GlowCard>
      )}

      {approveMutation.isError && (
        <GlowCard className="p-4 border-red-500/20">
          <div className="flex items-center gap-2 text-sm text-red-400">
            <AlertTriangle className="h-4 w-4" />
            {approveMutation.error instanceof Error
              ? approveMutation.error.message
              : "Failed to execute plan."}
          </div>
        </GlowCard>
      )}

      {/* Steps timeline */}
      <GlowCard className="p-0 overflow-hidden">
        <div className="p-4 border-b border-white/[0.06]">
          <h3 className="text-sm font-medium text-slate-300">Steps</h3>
        </div>

        <div className="p-4">
          <div className="relative">
            {/* Vertical timeline line */}
            <div className="absolute left-[9px] top-3 bottom-3 w-px bg-white/[0.06]" />

            <div className="space-y-1">
              {steps.map((step, i) => (
                <StaggerItem key={i} index={i} staggerMs={50}>
                  <div className="relative flex items-start gap-3 py-3 px-1">
                    {/* Icon */}
                    <div className="relative z-10 bg-nexus-deep">
                      {stepIcon(step.status)}
                    </div>

                    {/* Content */}
                    <div className="flex-1 min-w-0">
                      <div className="flex items-center gap-2">
                        <span className="text-sm font-medium text-slate-200">
                          {step.name}
                        </span>
                        {step.status && (
                          <Badge
                            className={cn(
                              "text-[10px] px-1.5 py-0",
                              step.status === "done" || step.status === "completed"
                                ? "bg-emerald-500/10 text-emerald-400 border-emerald-500/20"
                                : step.status === "in-progress" || step.status === "running"
                                  ? "bg-glow-cyan/10 text-glow-cyan border-glow-cyan/20"
                                  : "bg-white/[0.04] text-slate-500 border-white/[0.06]",
                            )}
                          >
                            {step.status}
                          </Badge>
                        )}
                      </div>
                      <p className="mt-0.5 text-xs text-slate-400 leading-relaxed">
                        {step.description}
                      </p>
                    </div>
                  </div>
                </StaggerItem>
              ))}
            </div>
          </div>
        </div>
      </GlowCard>
    </div>
  );
}
