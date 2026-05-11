"use client";

import { useState, useRef, useEffect } from "react";
import { useParams, useRouter } from "next/navigation";
import {
  ExternalLink,
  Play,
  Rocket,
  Sparkles,
  Square,
  FileCode,
  Share2,
  GitFork,
  Monitor,
  MessageSquare,
} from "lucide-react";
import { buildAppPreviewUrl, cn } from "@/lib/utils";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { StatusDot } from "@/components/ui/status-dot";
import { ProjectChat } from "@/components/project/chat";
import { NexusThinking } from "@/components/nexus-thinking";
import { AppPreview } from "@/components/app-preview";
import { useOneshotGeneration } from "@/hooks/use-oneshot-generation";
import { useProject } from "@/hooks/api/use-projects";
import { api } from "@/lib/api";
import { useToast } from "@/components/toast";
import { ShareProjectDialog } from "@/components/project/share-project-dialog";

const PHASE_LABELS: Record<number, string> = {
  1: "Exploring",
  2: "Structuring",
  3: "Building",
  4: "Published",
};

interface RuntimeStatus {
  running: boolean;
  port?: number;
  url?: string | null;
}

export default function ProjectPage() {
  const params = useParams();
  const router = useRouter();
  const projectId = params.projectId as string;

  const { data: project, isLoading } = useProject(projectId);
  const { toast } = useToast();
  const [runtime, setRuntime] = useState<RuntimeStatus>({ running: false });
  const [chatReloadKey, setChatReloadKey] = useState(0);
  const [shareOpen, setShareOpen] = useState(false);
  const [forking, setForking] = useState(false);

  async function handleFork() {
    if (forking) return;
    setForking(true);
    try {
      const forked = await api.forkProject(projectId);
      toast("success", "Project forked", `Opening ${forked.name}`);
      router.push(`/${forked.id}`);
    } catch (err) {
      toast("error", "Could not fork project", err instanceof Error ? err.message : String(err));
    } finally {
      setForking(false);
    }
  }

  const generation = useOneshotGeneration(projectId);
  const prevCompleteRef = useRef<string | null>(null);
  useEffect(() => {
    if (generation.isComplete && prevCompleteRef.current !== projectId) {
      prevCompleteRef.current = projectId;
      setChatReloadKey((k) => k + 1);
      if (typeof window !== "undefined" && window.location.search.includes("stream=1")) {
        window.history.replaceState({}, "", `/${projectId}`);
      }
      api.appStatus(projectId)
        .then((s) => { if (s) setRuntime({ running: s.status === "running", port: s.port }); })
        .catch(() => {});
      // Celebrate a successful build. Respect users who prefer reduced motion.
      if (
        typeof window !== "undefined" &&
        !window.matchMedia("(prefers-reduced-motion: reduce)").matches
      ) {
        import("canvas-confetti")
          .then(({ default: confetti }) => {
            const fire = (ratio: number, opts: Record<string, unknown>) =>
              confetti({
                particleCount: Math.floor(200 * ratio),
                spread: 70,
                origin: { y: 0.6 },
                colors: ["#22d3ee", "#6366f1", "#ec4899", "#f59e0b"],
                ...opts,
              });
            fire(0.25, { spread: 26, startVelocity: 55 });
            fire(0.2, { spread: 60 });
            fire(0.35, { spread: 100, decay: 0.91, scalar: 0.8 });
          })
          .catch(() => {});
      }
    }
  }, [generation.isComplete, projectId]);

  useEffect(() => {
    api.appStatus(projectId)
      .then((s) => { if (s) setRuntime({ running: s.status === "running", port: s.port }); })
      .catch(() => {});
  }, [projectId]);

  // Listen for global "stop generation" keyboard shortcut (Cmd/Ctrl+.)
  useEffect(() => {
    const onStop = () => {
      if (generation.active && !generation.isComplete && !generation.isError) {
        generation.abort();
        toast("info", "Generation stopped", "You can restart it from the chat.");
      }
    };
    window.addEventListener("nexus:stop-generation", onStop);
    return () => window.removeEventListener("nexus:stop-generation", onStop);
  }, [generation, toast]);

  const [starting, setStarting] = useState(false);
  // Mobile (< 768px): default to chat-only. `showPreviewMobile` toggles the
  // right pane; on md+ we show both panes regardless.
  const [showPreviewMobile, setShowPreviewMobile] = useState(false);

  async function handleStartApp() {
    setStarting(true);
    try {
      await api.startApp(projectId);
      for (let i = 0; i < 30; i++) {
        await new Promise((r) => setTimeout(r, 1000));
        const s = await api.appStatus(projectId).catch(() => null);
        if (s && s.status === "running" && s.port) {
          setRuntime({ running: true, port: s.port });
          // Preview is embedded inline in the split-pane below — reveal it on
          // mobile where it's hidden by default.
          setShowPreviewMobile(true);
          toast("success", "App started", `Running on port ${s.port}`);
          return;
        }
      }
      toast(
        "error",
        "App did not start in time",
        "Timed out after 30s. Check the build logs or try Preview again.",
      );
    } catch (err) {
      toast(
        "error",
        "Failed to start app",
        err instanceof Error ? err.message : String(err),
      );
    } finally {
      setStarting(false);
    }
  }

  return (
    <div className="flex flex-col h-full overflow-hidden page-enter">
      {/* Header */}
      <header className="flex items-center justify-between px-6 py-3 border-b border-white/[0.06] glass flex-shrink-0">
        <div className="flex items-center gap-3">
          <div className="w-9 h-9 rounded-lg bg-gradient-to-br from-glow-cyan/20 to-glow-blue/20 border border-glow-cyan/10 flex items-center justify-center">
            <Sparkles className="w-4 h-4 text-glow-cyan" />
          </div>
          <div>
            <div className="flex items-center gap-2">
              {isLoading ? (
                <Skeleton className="h-5 w-32" />
              ) : (
                <>
                  <h1 className="text-base font-semibold text-slate-200">
                    {project?.name ?? "Project"}
                  </h1>
                  {project && (
                    <Badge
                      variant={project.phase >= 3 ? "success" : project.phase >= 2 ? "info" : "secondary"}
                      className="text-[10px]"
                    >
                      {PHASE_LABELS[project.phase] ?? `Phase ${project.phase}`}
                    </Badge>
                  )}
                </>
              )}
            </div>
            <div className="flex items-center gap-2 mt-0.5">
              <StatusDot status={runtime.running ? "active" : "idle"} size="sm" />
              <span className="text-[11px] text-slate-400">
                {runtime.running ? `Running${runtime.port ? ` :${runtime.port}` : ""}` : "Stopped"}
              </span>
            </div>
          </div>
        </div>

        <div className="flex items-center gap-2">
          {runtime.running && runtime.port ? (
            <Button variant="outline" size="sm" asChild>
              <a
                href={buildAppPreviewUrl(runtime.port, runtime.url) ?? "#"}
                target="_blank"
                rel="noopener noreferrer"
                className="flex items-center gap-1.5"
              >
                <ExternalLink className="w-3.5 h-3.5" />
                Open App
              </a>
            </Button>
          ) : (
            <Button variant="outline" size="sm" onClick={handleStartApp} disabled={starting}>
              {starting ? (
                <>
                  <Sparkles className="w-3.5 h-3.5 mr-1.5 animate-pulse" />
                  Starting...
                </>
              ) : (
                <>
                  <Play className="w-3.5 h-3.5 mr-1.5" />
                  Preview
                </>
              )}
            </Button>
          )}
          <Button
            size="sm"
            variant="outline"
            onClick={handleFork}
            disabled={forking}
            aria-label="Fork project"
            title="Duplicate this project into a new one"
          >
            <GitFork className="w-3.5 h-3.5 mr-1.5" />
            {forking ? "Forking…" : "Fork"}
          </Button>
          <Button
            size="sm"
            variant="outline"
            onClick={() => setShareOpen(true)}
            aria-label="Share project"
          >
            <Share2 className="w-3.5 h-3.5 mr-1.5" />
            Share
          </Button>
          <Button
            size="sm"
            className="bg-gradient-to-r from-glow-cyan to-glow-blue hover:brightness-110 text-white"
            asChild
          >
            <a href={`/${projectId}/deploy`} className="flex items-center">
              <Rocket className="w-3.5 h-3.5 mr-1.5" />
              Deploy
            </a>
          </Button>
        </div>
      </header>
      <ShareProjectDialog open={shareOpen} onOpenChange={setShareOpen} project={project} />

      {/* Content */}
      <div className="flex-1 min-h-0 overflow-hidden">
        {generation.active && !generation.isComplete ? (
          <div className="h-full overflow-y-auto px-6 py-8">
            {generation.description && (
              <div className="mb-6 text-center max-w-2xl mx-auto">
                <div className="inline-flex items-center gap-2 px-4 py-2 rounded-full bg-gradient-to-r from-orange-500/10 via-pink-500/10 to-violet-500/10 border border-orange-500/20 mb-3 bg-[length:200%_100%] animate-[gradient-shift_6s_ease-in-out_infinite]">
                  <div className="w-2 h-2 rounded-full bg-orange-400 animate-pulse" />
                  <span className="text-xs font-medium bg-gradient-to-r from-orange-300 to-violet-300 bg-clip-text text-transparent">
                    Building your app
                  </span>
                </div>
                <p className="text-base text-slate-200 font-medium">
                  {generation.description}
                </p>
              </div>
            )}
            <GenerationActivityBar
              startedAt={generation.startedAt}
              lastEventAt={generation.lastEventAt}
              canAbort={!generation.isComplete && !generation.isError}
              onAbort={generation.abort}
            />
            <div className="max-w-6xl mx-auto grid grid-cols-1 lg:grid-cols-[1fr_320px] gap-6">
              <NexusThinking
                steps={generation.steps}
                progress={generation.progress}
                isComplete={generation.isComplete}
                isStreaming={!generation.isComplete && !generation.isError}
                onFixWithAi={async (step) => {
                  try {
                    const change = `Fix this error from the build: ${step.message}${step.detail ? " — " + step.detail : ""}`;
                    const res = await api.mutateProject(projectId, change);
                    if (res.applied) {
                      toast("success", "Fix applied", `${res.files_changed.length} file(s) updated`);
                    } else {
                      toast("warning", "AI proposed a fix but did not apply it", res.validation?.errors.join("; ") ?? "");
                    }
                  } catch (err) {
                    toast("error", "Could not apply fix", err instanceof Error ? err.message : String(err));
                  }
                }}
              />
              <GenerationFileTree steps={generation.steps} />
            </div>
          </div>
        ) : (
          <ProjectWorkspace
            projectId={projectId}
            chatReloadKey={chatReloadKey}
            runtime={runtime}
            appUrl={generation.appUrl}
            showPreviewMobile={showPreviewMobile}
            onToggleMobilePreview={() => setShowPreviewMobile((v) => !v)}
          />
        )}
      </div>
    </div>
  );
}

function GenerationActivityBar({
  startedAt,
  lastEventAt,
  canAbort,
  onAbort,
}: {
  startedAt: number | null;
  lastEventAt: number | null;
  canAbort?: boolean;
  onAbort?: () => void;
}) {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const t = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(t);
  }, []);
  if (!startedAt) return null;
  const elapsed = Math.max(0, Math.floor((now - startedAt) / 1000));
  const sinceEvent = lastEventAt ? Math.max(0, Math.floor((now - lastEventAt) / 1000)) : null;
  const stalled = sinceEvent != null && sinceEvent > 12;
  return (
    <div className="flex items-center justify-center gap-3 mb-4 text-xs text-slate-400">
      <span className="inline-flex items-center gap-1.5">
        <span
          className={cn(
            "h-1.5 w-1.5 rounded-full",
            stalled ? "bg-amber-400 animate-pulse" : "bg-emerald-400 animate-pulse",
          )}
        />
        {stalled ? "Still working -- long phase" : "Live"}
      </span>
      <span>&middot;</span>
      <span>{formatElapsed(elapsed)} elapsed</span>
      {sinceEvent != null && sinceEvent > 4 && (
        <>
          <span>&middot;</span>
          <span>{sinceEvent}s since last update</span>
        </>
      )}
      {canAbort && onAbort && (
        <>
          <span>&middot;</span>
          <button
            type="button"
            onClick={onAbort}
            aria-label="Stop generation"
            className="inline-flex items-center gap-1.5 px-2 py-0.5 rounded-md text-[11px] text-red-300 border border-red-500/20 bg-red-500/5 hover:bg-red-500/10 transition-colors"
          >
            <Square className="w-3 h-3" />
            Stop
          </button>
        </>
      )}
    </div>
  );
}

function GenerationFileTree({ steps }: { steps: { type: string; message: string; detail?: string; id: string }[] }) {
  // Extract "Wrote <path>" messages from file steps, dedup by path (keep latest).
  const byPath = new Map<string, { lines?: string; id: string }>();
  for (const s of steps) {
    if (s.type !== "file") continue;
    const m = /^Wrote\s+(.+)$/.exec(s.message);
    if (!m) continue;
    byPath.set(m[1], { lines: s.detail, id: s.id });
  }
  if (byPath.size === 0) {
    return (
      <aside className="hidden lg:block rounded-xl border border-white/[0.06] bg-white/[0.02] p-4 text-xs text-slate-500 sticky top-4 self-start">
        <div className="flex items-center gap-2 mb-2 text-slate-300">
          <FileCode className="w-3.5 h-3.5" />
          <span className="text-[11px] font-medium uppercase tracking-wider">Files</span>
        </div>
        Waiting for first file write…
      </aside>
    );
  }
  const entries = Array.from(byPath.entries());
  // Group by top-level folder for compactness.
  const groups = new Map<string, Array<{ path: string; lines?: string; latest: boolean }>>();
  const latestPath = entries[entries.length - 1][0];
  for (const [path, meta] of entries) {
    const idx = path.indexOf("/");
    const dir = idx > 0 ? path.slice(0, idx) : "/";
    const arr = groups.get(dir) ?? [];
    arr.push({ path, lines: meta.lines, latest: path === latestPath });
    groups.set(dir, arr);
  }
  return (
    <aside className="rounded-xl border border-white/[0.06] bg-white/[0.02] p-4 lg:sticky lg:top-4 self-start max-h-[70vh] overflow-y-auto">
      <div className="flex items-center justify-between mb-3">
        <div className="flex items-center gap-2 text-slate-300">
          <FileCode className="w-3.5 h-3.5" />
          <span className="text-[11px] font-medium uppercase tracking-wider">Files</span>
        </div>
        <span className="text-[10px] text-slate-500 tabular-nums">{byPath.size}</span>
      </div>
      <div className="space-y-3 text-[12px]">
        {Array.from(groups.entries()).map(([dir, files]) => (
          <div key={dir}>
            <div className="text-[10px] uppercase tracking-wider text-slate-500 mb-1">{dir}</div>
            <ul className="space-y-0.5">
              {files.map((f) => (
                <li
                  key={f.path}
                  className={cn(
                    "flex items-center justify-between gap-2 rounded px-1.5 py-0.5 font-mono",
                    f.latest ? "bg-glow-cyan/10 text-glow-cyan" : "text-slate-300",
                  )}
                >
                  <span className="truncate">{f.path}</span>
                  {f.lines && (
                    <span className="text-[10px] text-slate-500 tabular-nums flex-shrink-0">
                      {f.lines}
                    </span>
                  )}
                </li>
              ))}
            </ul>
          </div>
        ))}
      </div>
    </aside>
  );
}

function formatElapsed(s: number): string {
  if (s < 60) return `${s}s`;
  const m = Math.floor(s / 60);
  const r = s % 60;
  return `${m}m ${r}s`;
}

// ─────────────────────────────────────────────────────────────────────────────
// ProjectWorkspace — chat + inline preview split pane.
// ─────────────────────────────────────────────────────────────────────────────

interface ProjectWorkspaceProps {
  projectId: string;
  chatReloadKey: number;
  runtime: RuntimeStatus;
  appUrl: string | null;
  showPreviewMobile: boolean;
  onToggleMobilePreview: () => void;
}

function ProjectWorkspace({
  projectId,
  chatReloadKey,
  runtime,
  appUrl,
  showPreviewMobile,
  onToggleMobilePreview,
}: ProjectWorkspaceProps) {
  // The preview is only worth rendering once we have *somewhere* to point the
  // iframe: either an explicit app_url from the backend, or a known runtime
  // port. Before that, collapse the split into chat-only on every viewport.
  const previewUrl = buildAppPreviewUrl(runtime.port, appUrl);

  const canShowPreview = previewUrl != null;

  return (
    <div className="flex flex-col md:flex-row h-full w-full min-h-0">
      {/* Chat pane — full width on mobile (unless previewing), 40% on md+ */}
      <div
        className={cn(
          "min-h-0 flex flex-col border-white/[0.06]",
          // On mobile the chat hides when the user toggles the preview on.
          canShowPreview && showPreviewMobile ? "hidden md:flex" : "flex",
          canShowPreview ? "md:w-[40%] md:border-r" : "w-full",
        )}
      >
        {canShowPreview && (
          <div className="flex md:hidden items-center justify-end px-3 py-2 border-b border-white/[0.06]">
            <Button
              variant="outline"
              size="sm"
              onClick={onToggleMobilePreview}
              aria-label="Show preview"
            >
              <Monitor className="w-3.5 h-3.5 mr-1.5" />
              Show preview
            </Button>
          </div>
        )}
        <div className="flex-1 min-h-0">
          <ProjectChat key={chatReloadKey} projectId={projectId} />
        </div>
      </div>

      {/* Preview pane — 60% on md+, hidden until toggled on mobile */}
      {canShowPreview && (
        <div
          className={cn(
            "min-h-0 flex-col bg-[#0a0a0f]",
            showPreviewMobile ? "flex" : "hidden md:flex",
            "md:w-[60%] w-full",
          )}
        >
          <div className="flex md:hidden items-center justify-between px-3 py-2 border-b border-white/[0.06]">
            <span className="text-xs text-slate-400">Live preview</span>
            <Button
              variant="ghost"
              size="sm"
              onClick={onToggleMobilePreview}
              aria-label="Hide preview and return to chat"
            >
              <MessageSquare className="w-3.5 h-3.5 mr-1.5" />
              Back to chat
            </Button>
          </div>
          <div className="flex-1 min-h-0 overflow-auto p-3">
            <AppPreview
              url={previewUrl}
              port={runtime.port ?? 0}
              // Treat any resolvable preview URL as "running" so the iframe
              // mounts — the AppPreview toolbar already shows a live indicator
              // based on this prop.
              status={runtime.running || previewUrl ? "running" : "stopped"}
              className="h-full"
            />
          </div>
        </div>
      )}
    </div>
  );
}
