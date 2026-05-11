"use client";

import { useState } from "react";
import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import {
  Settings,
  Calendar,
  Shield,
  Trash2,
  AlertTriangle,
  RefreshCw,
} from "lucide-react";
import { cn } from "@/lib/utils";
import { api } from "@/lib/api";
import {
  Card,
  CardHeader,
  CardTitle,
  CardDescription,
  CardContent,
} from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Input } from "@/components/ui/input";
import { Skeleton } from "@/components/ui/skeleton";
import { Separator } from "@/components/ui/separator";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
} from "@/components/ui/dialog";
import { useToast } from "@/components/toast";
import { useRouter } from "next/navigation";

const PHASE_LABELS: Record<number, { label: string; variant: "default" | "info" | "warning" | "success" | "purple" }> = {
  0: { label: "Initialized", variant: "secondary" as never },
  1: { label: "Planning", variant: "info" },
  2: { label: "Designing", variant: "purple" },
  3: { label: "Building", variant: "warning" },
  4: { label: "Testing", variant: "default" },
  5: { label: "Deployed", variant: "success" },
};

function formatDate(iso: string): string {
  const d = new Date(iso);
  return d.toLocaleDateString(undefined, {
    year: "numeric",
    month: "long",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
}

function SettingsSkeleton() {
  return (
    <div className="h-full overflow-y-auto scrollbar-thin p-6 space-y-6">
      {[1, 2, 3].map((i) => (
        <Card key={i}>
          <CardHeader>
            <Skeleton className="h-5 w-40" />
            <Skeleton className="h-4 w-64 mt-1" />
          </CardHeader>
          <CardContent className="space-y-4">
            <Skeleton className="h-4 w-full" />
            <Skeleton className="h-4 w-3/4" />
            <Skeleton className="h-4 w-1/2" />
          </CardContent>
        </Card>
      ))}
    </div>
  );
}

function ErrorState({ message, onRetry }: { message: string; onRetry: () => void }) {
  return (
    <div className="h-full flex items-center justify-center p-6">
      <Card className="max-w-md w-full">
        <CardContent className="pt-6 text-center space-y-4">
          <div className="mx-auto w-12 h-12 rounded-full bg-destructive/10 flex items-center justify-center">
            <AlertTriangle className="w-6 h-6 text-destructive" />
          </div>
          <div>
            <p className="text-sm font-medium text-slate-200">Failed to load settings</p>
            <p className="text-xs text-slate-400 mt-1">{message}</p>
          </div>
          <Button variant="outline" size="sm" onClick={onRetry}>
            <RefreshCw className="w-3.5 h-3.5 mr-2" />
            Retry
          </Button>
        </CardContent>
      </Card>
    </div>
  );
}

export function ProjectSettings({ projectId }: { projectId: string }) {
  const router = useRouter();
  const queryClient = useQueryClient();
  const { toast } = useToast();

  const [deleteDialogOpen, setDeleteDialogOpen] = useState(false);
  const [confirmName, setConfirmName] = useState("");

  const projectQuery = useQuery({
    queryKey: ["project", projectId],
    queryFn: () => api.getProject(projectId),
  });

  const sandboxQuery = useQuery({
    queryKey: ["sandbox-settings"],
    queryFn: () => api.getSandboxSettings(),
  });

  const sandboxMutation = useMutation({
    mutationFn: (enabled: boolean) => api.setSandboxSettings(enabled),
    onSuccess: (_, enabled) => {
      queryClient.invalidateQueries({ queryKey: ["sandbox-settings"] });
      toast("success", `Sandbox ${enabled ? "enabled" : "disabled"}`);
    },
    onError: () => {
      toast("error", "Failed to update sandbox settings");
    },
  });

  const deleteMutation = useMutation({
    mutationFn: () => api.deleteProject(projectId),
    onSuccess: () => {
      toast("success", "Project deleted");
      queryClient.invalidateQueries({ queryKey: ["projects"] });
      router.push("/");
    },
    onError: () => {
      toast("error", "Failed to delete project");
    },
  });

  if (projectQuery.isLoading || sandboxQuery.isLoading) {
    return <SettingsSkeleton />;
  }

  if (projectQuery.isError) {
    return (
      <ErrorState
        message={projectQuery.error?.message ?? "Unknown error"}
        onRetry={() => projectQuery.refetch()}
      />
    );
  }

  const project = projectQuery.data!;
  const sandbox = sandboxQuery.data;
  const phaseInfo = PHASE_LABELS[project.phase] ?? { label: `Phase ${project.phase}`, variant: "secondary" as const };
  const canDelete = confirmName === project.name;

  return (
    <div className="h-full overflow-y-auto scrollbar-thin p-6 space-y-6">
      {/* General */}
      <Card>
        <CardHeader>
          <div className="flex items-center gap-2">
            <Settings className="w-4 h-4 text-glow-cyan" />
            <CardTitle className="text-base">General</CardTitle>
          </div>
          <CardDescription>Basic project information and metadata.</CardDescription>
        </CardHeader>
        <CardContent className="space-y-4">
          <div className="grid grid-cols-2 gap-4">
            <div className="space-y-1">
              <p className="text-xs font-medium text-slate-400 uppercase tracking-wider">Name</p>
              <p className="text-sm text-slate-200">{project.name}</p>
            </div>
            <div className="space-y-1">
              <p className="text-xs font-medium text-slate-400 uppercase tracking-wider">Phase</p>
              <Badge variant={phaseInfo.variant as "default"}>{phaseInfo.label}</Badge>
            </div>
          </div>

          {project.description && (
            <div className="space-y-1">
              <p className="text-xs font-medium text-slate-400 uppercase tracking-wider">Description</p>
              <p className="text-sm text-slate-300 leading-relaxed">{project.description}</p>
            </div>
          )}

          <Separator />

          <div className="grid grid-cols-2 gap-4">
            <div className="space-y-1">
              <p className="text-xs font-medium text-slate-400 uppercase tracking-wider">Created</p>
              <div className="flex items-center gap-1.5 text-sm text-slate-300">
                <Calendar className="w-3.5 h-3.5 text-slate-500" />
                {formatDate(project.created_at)}
              </div>
            </div>
            <div className="space-y-1">
              <p className="text-xs font-medium text-slate-400 uppercase tracking-wider">Last Updated</p>
              <div className="flex items-center gap-1.5 text-sm text-slate-300">
                <Calendar className="w-3.5 h-3.5 text-slate-500" />
                {formatDate(project.updated_at)}
              </div>
            </div>
          </div>

          {(project.llm_provider || project.llm_model) && (
            <>
              <Separator />
              <div className="grid grid-cols-2 gap-4">
                {project.llm_provider && (
                  <div className="space-y-1">
                    <p className="text-xs font-medium text-slate-400 uppercase tracking-wider">LLM Provider</p>
                    <p className="text-sm text-slate-300">{project.llm_provider}</p>
                  </div>
                )}
                {project.llm_model && (
                  <div className="space-y-1">
                    <p className="text-xs font-medium text-slate-400 uppercase tracking-wider">LLM Model</p>
                    <p className="text-sm text-slate-300">{project.llm_model}</p>
                  </div>
                )}
              </div>
            </>
          )}
        </CardContent>
      </Card>

      {/* Runtime */}
      <Card>
        <CardHeader>
          <div className="flex items-center gap-2">
            <Shield className="w-4 h-4 text-glow-cyan" />
            <CardTitle className="text-base">Runtime</CardTitle>
          </div>
          <CardDescription>
            Control how code is executed within this project.
          </CardDescription>
        </CardHeader>
        <CardContent>
          <div className="flex items-center justify-between rounded-lg border border-white/[0.06] bg-white/[0.02] p-4">
            <div className="space-y-1">
              <p className="text-sm font-medium text-slate-200">Sandbox Mode</p>
              <p className="text-xs text-slate-400">
                Run generated code in an isolated sandbox environment for safety.
              </p>
            </div>
            <button
              type="button"
              role="switch"
              aria-checked={sandbox?.enabled ?? false}
              disabled={sandboxMutation.isPending}
              onClick={() => sandboxMutation.mutate(!(sandbox?.enabled ?? false))}
              className={cn(
                "relative inline-flex h-6 w-11 shrink-0 cursor-pointer rounded-full border-2 border-transparent transition-colors duration-200 focus-visible:outline-none focus-visible:shadow-glow-sm disabled:cursor-not-allowed disabled:opacity-50",
                sandbox?.enabled
                  ? "bg-glow-cyan"
                  : "bg-white/[0.1]"
              )}
            >
              <span
                className={cn(
                  "pointer-events-none inline-block h-5 w-5 rounded-full bg-white shadow-lg transition-transform duration-200",
                  sandbox?.enabled ? "translate-x-5" : "translate-x-0"
                )}
              />
            </button>
          </div>
        </CardContent>
      </Card>

      {/* Danger Zone */}
      <Card className="border-destructive/20">
        <CardHeader>
          <div className="flex items-center gap-2">
            <AlertTriangle className="w-4 h-4 text-destructive" />
            <CardTitle className="text-base text-destructive">Danger Zone</CardTitle>
          </div>
          <CardDescription>
            Irreversible actions that permanently affect this project.
          </CardDescription>
        </CardHeader>
        <CardContent>
          <div className="flex items-center justify-between rounded-lg border border-destructive/20 bg-destructive/[0.04] p-4">
            <div className="space-y-1">
              <p className="text-sm font-medium text-slate-200">Delete Project</p>
              <p className="text-xs text-slate-400">
                Permanently remove this project and all associated data. This cannot be undone.
              </p>
            </div>
            <Button
              variant="destructive"
              size="sm"
              onClick={() => {
                setConfirmName("");
                setDeleteDialogOpen(true);
              }}
            >
              <Trash2 className="w-3.5 h-3.5 mr-2" />
              Delete
            </Button>
          </div>
        </CardContent>
      </Card>

      {/* Delete Confirmation Dialog */}
      <Dialog open={deleteDialogOpen} onOpenChange={setDeleteDialogOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle className="text-destructive">Delete Project</DialogTitle>
            <DialogDescription>
              This action is permanent and cannot be undone. All project data, files,
              agents, and workflows will be permanently deleted.
            </DialogDescription>
          </DialogHeader>
          <div className="space-y-4 pt-2">
            <div className="space-y-2">
              <p className="text-sm text-slate-300">
                Type <span className="font-mono font-semibold text-slate-100">{project.name}</span> to confirm:
              </p>
              <Input
                value={confirmName}
                onChange={(e) => setConfirmName(e.target.value)}
                placeholder={project.name}
                autoComplete="off"
                autoFocus
              />
            </div>
            <div className="flex justify-end gap-2">
              <Button
                variant="outline"
                size="sm"
                onClick={() => setDeleteDialogOpen(false)}
              >
                Cancel
              </Button>
              <Button
                variant="destructive"
                size="sm"
                disabled={!canDelete || deleteMutation.isPending}
                onClick={() => deleteMutation.mutate()}
              >
                {deleteMutation.isPending ? (
                  <>
                    <RefreshCw className="w-3.5 h-3.5 mr-2 animate-spin" />
                    Deleting...
                  </>
                ) : (
                  <>
                    <Trash2 className="w-3.5 h-3.5 mr-2" />
                    Delete Project
                  </>
                )}
              </Button>
            </div>
          </div>
        </DialogContent>
      </Dialog>
    </div>
  );
}
