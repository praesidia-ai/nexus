"use client";

import { useState, useEffect, useRef, useCallback, useMemo } from "react";
import { useSearchParams, useRouter } from "next/navigation";
import { Send, Globe, Check, ExternalLink, Square, Sparkles, Database, Bot, Workflow, Zap, Users, Play, FileCode, Loader2 } from "lucide-react";
import { buildAppPreviewUrl, cn } from "@/lib/utils";
import { api, streamChat, type NexusMessage, type ChatEvent } from "@/lib/api";
import { useToast } from "@/components/toast";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { ModelSelector } from "@/components/model-selector";
import { WorkflowDesignCard } from "@/components/project/workflow-design-card";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface ActionCard {
  action: string;
  data: Record<string, unknown>;
  success: boolean;
}

interface Message {
  id: string;
  role: "user" | "assistant";
  content: string;
  streaming?: boolean;
  actionCards?: ActionCard[];
  phase?: number;
  phase_label?: string;
  milestone?: string;
}

// ---------------------------------------------------------------------------
// Thinking bubble
// ---------------------------------------------------------------------------

const THINKING_STEPS = [
  "Thinking...",
  "Reading between the lines...",
  "Considering the possibilities...",
  "Exploring creative paths...",
  "Connecting the dots...",
  "Digging into the details...",
  "Zooming out for perspective...",
  "Following the thread...",
  "Untangling the problem...",
  "Seeing the bigger picture...",
  "Analyzing your request...",
  "Mapping the knowledge graph...",
  "Crafting the response...",
  "Processing context...",
  "Building the solution...",
  "Assembling the pieces...",
  "Structuring the answer...",
  "Reasoning through the logic...",
  "Cross-referencing everything...",
  "Crunching the details...",
  "Consulting the oracle...",
  "Asking the right questions...",
  "Making neurons fire...",
  "Brewing something good...",
  "Almost there...",
  "Spinning up the engines...",
  "Running at full speed...",
  "Synthesizing brilliance...",
  "Summoning creativity...",
  "Unlocking potential...",
  "Mixing logic with imagination...",
  "Sharpening the pencil...",
  "Taking it one step at a time...",
  "Checking all the boxes...",
  "Thinking like a founder...",
];

function pickRandom<T>(arr: T[], exclude?: T): T {
  const pool = exclude !== undefined ? arr.filter((x) => x !== exclude) : arr;
  return pool[Math.floor(Math.random() * pool.length)];
}

function ThinkingBubble() {
  const [step, setStep] = useState(() => pickRandom(THINKING_STEPS));
  const [visible, setVisible] = useState(true);

  useEffect(() => {
    let timeoutId: ReturnType<typeof setTimeout> | null = null;
    const interval = setInterval(() => {
      setVisible(false);
      timeoutId = setTimeout(() => {
        setStep((prev) => pickRandom(THINKING_STEPS, prev));
        setVisible(true);
      }, 250);
    }, 1800 + Math.random() * 800);
    return () => {
      clearInterval(interval);
      if (timeoutId) clearTimeout(timeoutId);
    };
  }, []);

  return (
    <div className="flex justify-start mb-3">
      <div className="glass-card neon-glow-purple px-4 py-3 rounded-2xl flex items-center gap-3">
        <div className="relative w-5 h-5 flex-shrink-0">
          <div className="absolute inset-0 rounded-full border-2 border-glow-cyan/30 border-t-primary animate-spin" />
        </div>
        <span
          className={cn(
            "text-[13px] text-slate-400 transition-opacity duration-250 min-w-[180px]",
            visible ? "opacity-100" : "opacity-0"
          )}
        >
          {step}
        </span>
        <span className="flex gap-0.5 flex-shrink-0">
          {[0, 1, 2].map((i) => (
            <span
              key={i}
              className="w-1 h-1 rounded-full bg-glow-cyan"
              style={{ animation: `projectChatBounce 1.2s ease-in-out ${i * 0.2}s infinite` }}
            />
          ))}
        </span>
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Processing step indicator
// ---------------------------------------------------------------------------

const ACTION_ICONS: Record<string, React.ReactNode> = {
  create_table: <Database className="w-3.5 h-3.5" />,
  create_agent: <Bot className="w-3.5 h-3.5" />,
  create_workflow: <Workflow className="w-3.5 h-3.5" />,
  create_portal: <Globe className="w-3.5 h-3.5" />,
  design_agent_workflow: <Users className="w-3.5 h-3.5" />,
};

const ACTION_PROCESSING_LABELS: Record<string, (data: Record<string, unknown>) => string> = {
  create_table: (d) => `Creating table ${d.table_name ?? ""}...`,
  create_agent: (d) => `Configuring agent ${d.agent_name ?? ""}...`,
  create_workflow: (d) => `Setting up workflow ${d.name ?? ""}...`,
  create_portal: (d) => `Building portal ${d.slug ? `/${d.slug}` : ""}...`,
  design_agent_workflow: () => `Designing agent workflow...`,
};

// ---------------------------------------------------------------------------
// Portal card
// ---------------------------------------------------------------------------

function PortalCard({
  data,
  projectId,
}: {
  data: Record<string, unknown>;
  projectId: string;
}) {
  const { toast } = useToast();
  const [publishing, setPublishing] = useState(false);
  const [published, setPublished] = useState(false);

  const slug = (data.slug as string) ?? "";
  const portalUrl = `/api/portal/${slug}`;

  const features = [
    "Login with email authentication",
    "Dashboard: active & completed projects",
    "Submit new requests",
    "Download completed files",
    "Real-time status updates",
  ];

  async function publish() {
    setPublishing(true);
    try {
      await api.publishPortal(projectId, {
        project_name: (data.project_name as string) ?? "My Portal",
        agent_name: (data.agent_name as string) ?? "Assistant",
        slug,
      });
      setPublished(true);
    } catch (e) {
      const msg = e instanceof Error ? e.message : "Failed to publish portal";
      toast("error", "Publish failed", msg);
    } finally {
      setPublishing(false);
    }
  }

  return (
    <div className="mt-3 rounded-xl border border-white/[0.08] overflow-hidden w-80">
      <div className="bg-gradient-to-r from-glow-cyan to-glow-blue px-4 py-2.5 flex items-center gap-2">
        <Globe className="w-4 h-4 text-white" />
        <span className="text-white font-semibold text-sm">Client Portal</span>
        <span className="ml-1 text-cyan-200 text-xs">{slug ? `portal.${slug}.com` : ""}</span>
      </div>
      <div className="glass-card px-4 py-3 border-b border-white/[0.06] rounded-none">
        {features.map((f) => (
          <div key={f} className="flex items-center gap-2 py-0.5">
            <Check className="w-3.5 h-3.5 text-emerald-400 flex-shrink-0" />
            <span className="text-[12px] text-slate-400">{f}</span>
          </div>
        ))}
      </div>
      <div className="px-4 py-3 flex gap-2 bg-black/20 backdrop-blur-sm">
        <Button
          onClick={publish}
          disabled={publishing || published}
          className={cn(
            "flex-1 text-[13px] font-semibold",
            published && "bg-emerald-500 hover:bg-emerald-600"
          )}
        >
          {published ? "Published!" : publishing ? "Publishing..." : "Publish Portal"}
        </Button>
        {slug && (
          <Button variant="outline" asChild>
            <a
              href={portalUrl}
              target="_blank"
              rel="noopener noreferrer"
              className="text-[13px] flex items-center gap-1"
            >
              Preview <ExternalLink className="w-3 h-3" />
            </a>
          </Button>
        )}
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Action result card
// ---------------------------------------------------------------------------

function ActionResultCard({ card }: { card: ActionCard }) {
  if (!card.success) return null;

  if (card.action === "design_agent_workflow") {
    return <WorkflowDesignCard data={card.data} />;
  }

  const icons: Record<string, string> = {
    create_table: "🗃️",
    create_agent: "🤖",
    create_workflow: "⚙️",
    create_portal: "🌐",
  };

  const labels: Record<string, string> = {
    create_table: `Created table: ${card.data.table_name ?? ""}`,
    create_agent: `Created agent: ${card.data.agent_name ?? ""}`,
    create_workflow: `Created workflow: ${card.data.name ?? ""}`,
    create_portal: `Portal ready: /${card.data.slug ?? ""}`,
  };

  return (
    <div className="mt-2 flex items-center gap-2 px-3 py-2 rounded-lg bg-emerald-500/10 border border-emerald-500/20 text-[12px] text-emerald-400 w-fit">
      <span>{icons[card.action] ?? "✓"}</span>
      <span>{labels[card.action] ?? card.action}</span>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Processing steps inline strip
// ---------------------------------------------------------------------------

interface ProcessingStepsProps {
  steps: { action: string; data: Record<string, unknown>; done: boolean }[];
}

function ProcessingSteps({ steps }: ProcessingStepsProps) {
  if (steps.length === 0) return null;
  return (
    <div className="mt-3 space-y-1.5">
      {steps.map((step, i) => (
        <div
          key={`${step.action}-${i}`}
          className={cn(
            "flex items-center gap-2 text-[12px] px-3 py-1.5 rounded-lg border w-fit transition-all",
            step.done
              ? "bg-emerald-500/10 border-emerald-500/20 text-emerald-400"
              : "bg-glow-cyan/10 border-glow-cyan/20 text-glow-cyan"
          )}
        >
          <span
            className={cn(
              "flex-shrink-0",
              !step.done && "animate-pulse"
            )}
          >
            {step.done ? (
              <Check className="w-3.5 h-3.5 text-emerald-400" />
            ) : (
              <Zap className="w-3.5 h-3.5 text-glow-cyan" />
            )}
          </span>
          <span className={cn("flex items-center gap-1.5", ACTION_ICONS[step.action] ? "" : "")}>
            {ACTION_ICONS[step.action] && (
              <span className="opacity-60">{ACTION_ICONS[step.action]}</span>
            )}
            {step.done
              ? (() => {
                  const labels: Record<string, string> = {
                    create_table: `Table created: ${step.data.table_name ?? ""}`,
                    create_agent: `Agent ready: ${step.data.agent_name ?? ""}`,
                    create_workflow: `Workflow set up: ${step.data.name ?? ""}`,
                    create_portal: `Portal built: /${step.data.slug ?? ""}`,
                  };
                  return labels[step.action] ?? step.action;
                })()
              : (ACTION_PROCESSING_LABELS[step.action]?.(step.data) ?? `${step.action}...`)}
          </span>
        </div>
      ))}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Message bubble
// ---------------------------------------------------------------------------

const BUILD_COMPLETE_RE = /^Built .+ — generated \d+ files? in /;

function BuildCompleteActions({
  projectId,
}: {
  projectId: string;
}) {
  const router = useRouter();
  const [starting, setStarting] = useState(false);
  const [appPort, setAppPort] = useState<number | null>(null);
  const [startError, setStartError] = useState<string | null>(null);

  useEffect(() => {
    api.appStatus(projectId)
      .then((s) => { if (s?.status === "running" && s.port) setAppPort(s.port); })
      .catch(() => {});
  }, [projectId]);

  async function handlePreview() {
    if (appPort) {
      const previewUrl = buildAppPreviewUrl(appPort);
      if (previewUrl) {
        window.open(previewUrl, "_blank", "noopener,noreferrer");
      } else {
        setStartError("App is running but preview URL is unavailable.");
      }
      return;
    }
    setStarting(true);
    setStartError(null);
    try {
      await api.startApp(projectId, true);
      for (let i = 0; i < 60; i++) {
        await new Promise((r) => setTimeout(r, 1000));
        const s = await api.appStatus(projectId).catch(() => null);
        if (s?.status === "running" && s.port) {
          setAppPort(s.port);
          const previewUrl = buildAppPreviewUrl(s.port, s.url);
          if (previewUrl) {
            window.open(previewUrl, "_blank", "noopener,noreferrer");
          } else {
            setStartError("App started but preview URL could not be built.");
          }
          return;
        }
        if (s?.status === "failed" || s?.status === "crashed") {
          setStartError(`App failed to start (status: ${s.status}). Check logs.`);
          return;
        }
      }
      setStartError("App did not start within 60s. Check logs.");
    } catch (err) {
      setStartError(err instanceof Error ? err.message : "Failed to start app");
    } finally {
      setStarting(false);
    }
  }

  return (
    <div className="mt-3 pt-3 border-t border-white/[0.06] flex flex-wrap gap-2">
      <button
        onClick={handlePreview}
        disabled={starting}
        className="inline-flex items-center gap-1.5 px-3.5 py-2 rounded-lg text-[13px] font-medium bg-gradient-to-r from-glow-cyan/20 to-glow-blue/20 border border-glow-cyan/20 text-glow-cyan hover:from-glow-cyan/30 hover:to-glow-blue/30 transition-all"
      >
        {starting ? (
          <Loader2 className="w-3.5 h-3.5 animate-spin" />
        ) : appPort ? (
          <ExternalLink className="w-3.5 h-3.5" />
        ) : (
          <Play className="w-3.5 h-3.5" />
        )}
        {starting ? "Starting..." : appPort ? "Open App" : "Preview App"}
      </button>
      <button
        onClick={() => router.push(`/${projectId}/files`)}
        className="inline-flex items-center gap-1.5 px-3.5 py-2 rounded-lg text-[13px] font-medium bg-white/[0.04] border border-white/[0.08] text-slate-300 hover:bg-white/[0.08] hover:text-slate-100 transition-all"
      >
        <FileCode className="w-3.5 h-3.5" />
        View Files
      </button>
      {startError && (
        <p className="basis-full text-xs text-red-400 mt-1">{startError}</p>
      )}
    </div>
  );
}

function MessageBubble({
  msg,
  projectId,
  processingSteps,
}: {
  msg: Message;
  projectId: string;
  processingSteps?: { action: string; data: Record<string, unknown>; done: boolean }[];
}) {
  if (msg.role === "user") {
    return (
      <div className="flex justify-end mb-3">
        <div className="max-w-[70%] px-4 py-2.5 rounded-2xl bg-glow-cyan text-white text-[14px] leading-relaxed shadow-[0_0_10px_rgba(249,115,22,0.1)]">
          {msg.content}
        </div>
      </div>
    );
  }

  if (msg.streaming && msg.content === "") {
    return (
      <div>
        <ThinkingBubble />
        {processingSteps && processingSteps.length > 0 && (
          <div className="flex justify-start mb-3">
            <div className="glass-card px-4 py-3 rounded-2xl">
              <ProcessingSteps steps={processingSteps} />
            </div>
          </div>
        )}
      </div>
    );
  }

  const hasPortalCard = msg.actionCards?.some((c) => c.action === "create_portal");
  const portalCard = msg.actionCards?.find((c) => c.action === "create_portal");
  const otherCards = msg.actionCards?.filter((c) => c.action !== "create_portal") ?? [];
  const isBuildComplete = !msg.streaming && BUILD_COMPLETE_RE.test(msg.content);

  return (
    <div className="flex justify-start mb-3">
      <div className="max-w-[75%]">
        <div className="flex items-center gap-1.5 mb-1.5 ml-1">
          <div className="w-5 h-5 rounded-full bg-gradient-to-br from-glow-cyan to-glow-blue flex items-center justify-center shadow-glow-sm">
            <Sparkles className="w-3 h-3 text-white" />
          </div>
          <span className="text-[11px] text-slate-400 font-medium">Nexus</span>
          {msg.phase_label && msg.phase && msg.phase > 1 && (
            <Badge
              variant={
                msg.phase === 2
                  ? "info"
                  : msg.phase === 3
                  ? "purple"
                  : msg.phase === 4
                  ? "success"
                  : "default"
              }
              className="text-[10px] ml-1"
            >
              {msg.phase_label}
            </Badge>
          )}
        </div>

        <div className="glass-card px-4 py-3 rounded-2xl">
          <p className="text-[14px] text-slate-200 leading-relaxed whitespace-pre-wrap">
            {msg.content}
            {msg.streaming && (
              <span className="inline-block w-[3px] h-4 bg-glow-cyan rounded-sm ml-0.5 animate-pulse align-middle" />
            )}
          </p>

          {isBuildComplete && (
            <BuildCompleteActions projectId={projectId} />
          )}

          {msg.milestone && (
            <div className="mt-2 pt-2 border-t border-white/[0.06] text-[11px] text-slate-400 flex items-center gap-1">
              <Check className="w-3 h-3 text-emerald-400" />
              {msg.milestone}
            </div>
          )}

          {msg.streaming && processingSteps && processingSteps.length > 0 && (
            <ProcessingSteps steps={processingSteps} />
          )}
        </div>

        {hasPortalCard && portalCard && (
          <PortalCard data={portalCard.data} projectId={projectId} />
        )}

        {otherCards.map((card, i) => (
          <ActionResultCard key={`${card.action}-${i}`} card={card} />
        ))}
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// ProjectChat component
// ---------------------------------------------------------------------------

const CHAT_PLACEHOLDERS = [
  "Type a message...",
  "Ask me anything...",
  "What should we build next?",
  "Tell me what you need...",
  "Describe the next step...",
  "What's on your mind?",
  "Continue the conversation...",
  "Ask away...",
];

const EMPTY_CHAT_MESSAGES = [
  "Start a conversation to build your business infrastructure",
  "Ask me anything -- I'll help you build, automate, and launch",
  "Describe your idea and I'll turn it into a working system",
  "Tell me what you need and I'll get to work",
  "What can I help you build today?",
];

export function ProjectChat({ projectId }: { projectId: string }) {
  const searchParams = useSearchParams();
  const { toast } = useToast();

  const chatPlaceholder = useMemo(
    () => CHAT_PLACEHOLDERS[Math.floor(Math.random() * CHAT_PLACEHOLDERS.length)],
    []
  );
  const emptyChatMessage = useMemo(
    () => EMPTY_CHAT_MESSAGES[Math.floor(Math.random() * EMPTY_CHAT_MESSAGES.length)],
    []
  );

  const [messages, setMessages] = useState<Message[]>([]);
  const [input, setInput] = useState("");
  const [conversationId, setConversationId] = useState<string | undefined>();
  const [streaming, setStreaming] = useState(false);
  const [chatProvider, setChatProvider] = useState<string | undefined>();
  const [chatModel, setChatModel] = useState<string | undefined>();
  const projectModelLoaded = useRef(false);
  const [processingSteps, setProcessingSteps] = useState<
    { action: string; data: Record<string, unknown>; done: boolean }[]
  >([]);
  const currentAssistantId = useRef<string | null>(null);
  const bottomRef = useRef<HTMLDivElement>(null);
  const abortRef = useRef<AbortController | null>(null);
  const sendMessageRef = useRef<((text: string) => Promise<void>) | null>(null);
  const firstMessageFired = useRef(false);

  useEffect(() => {
    if (projectModelLoaded.current) return;
    projectModelLoaded.current = true;
    (async () => {
      // Resolution order: project override → global default → first configured
      // provider's first model. This guarantees the dropdown is always populated
      // when at least one API key exists in Settings.
      try {
        const m = await api.getProjectModel(projectId);
        if (m.provider && m.model) {
          setChatProvider(m.provider);
          setChatModel(m.model);
          return;
        }
      } catch { /* fall through */ }
      try {
        const d = await api.getDefaultModel();
        if (d.provider && d.model) {
          setChatProvider(d.provider);
          setChatModel(d.model);
          return;
        }
      } catch { /* fall through */ }
      try {
        const [providers, keys] = await Promise.all([
          api.listProviders(),
          api.listApiKeys(),
        ]);
        const configuredIds = new Set(
          (keys.providers ?? []).filter((p) => p.configured).map((p) => p.provider),
        );
        const provider = providers.find((p) => configuredIds.has(p.id) && p.models.length > 0);
        if (provider) {
          setChatProvider(provider.id);
          setChatModel(provider.models[0].id);
        }
      } catch { /* ignore — dropdown will show empty and user can pick manually */ }
    })();
  }, [projectId]);

  useEffect(() => {
    const firstMsg = searchParams.get("firstMessage");
    async function load() {
      let hasHistory = false;
      try {
        const convs = await api.listConversations(projectId);
        if (convs.length > 0) {
          const latestConv = convs[convs.length - 1];
          setConversationId(latestConv.id);
          const msgs = await api.listMessages(projectId, latestConv.id);
          if (msgs.length > 0) {
            hasHistory = true;
            setMessages(
              msgs.map((m: NexusMessage) => ({
                id: m.id,
                role: m.role as "user" | "assistant",
                content: m.content,
              }))
            );
          }
        }
      } catch {
        // no conversations yet
      }
      if (firstMsg && !hasHistory && !firstMessageFired.current) {
        firstMessageFired.current = true;
        window.history.replaceState({}, "", `/${projectId}`);
        sendMessageRef.current?.(firstMsg);
      }
    }
    load();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [projectId]);

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [messages, processingSteps]);

  const sendMessage = useCallback(
    async (text: string) => {
      if (!text.trim() || streaming) return;

      const userMsg: Message = {
        id: crypto.randomUUID(),
        role: "user",
        content: text.trim(),
      };
      const assistantMsg: Message = {
        id: crypto.randomUUID(),
        role: "assistant",
        content: "",
        streaming: true,
        actionCards: [],
      };

      currentAssistantId.current = assistantMsg.id;
      setMessages((prev) => [...prev, userMsg, assistantMsg]);
      setInput("");
      setStreaming(true);
      setProcessingSteps([]);

      abortRef.current = new AbortController();

      try {
        await streamChat(
          projectId,
          text.trim(),
          conversationId,
          (event: ChatEvent) => {
            if (event.type === "token") {
              setMessages((prev) =>
                prev.map((m) =>
                  m.id === assistantMsg.id
                    ? { ...m, content: m.content + (event.content ?? "") }
                    : m
                )
              );
            } else if (event.type === "action_result") {
              const actionName = event.action ?? "";
              const data = event.data ?? {};
              setProcessingSteps((prev) => {
                const idx = prev.findIndex(
                  (s) => s.action === actionName && !s.done
                );
                if (idx !== -1) {
                  const next = [...prev];
                  next[idx] = { ...next[idx], done: true };
                  return next;
                }
                return [...prev, { action: actionName, data, done: true }];
              });

              if (event.success !== false) {
                const card: ActionCard = {
                  action: actionName,
                  data,
                  success: event.success ?? true,
                };
                setMessages((prev) =>
                  prev.map((m) =>
                    m.id === assistantMsg.id
                      ? { ...m, actionCards: [...(m.actionCards ?? []), card] }
                      : m
                  )
                );
              }
            } else if (event.type === "error") {
              // Previously dropped silently — bubble up so the user sees why
              // their message failed instead of an infinite "streaming" pill.
              const errMsg =
                (event as { message?: string }).message ?? "Chat stream reported an error";
              setMessages((prev) =>
                prev.map((m) =>
                  m.id === assistantMsg.id
                    ? { ...m, content: errMsg, streaming: false }
                    : m
                )
              );
              toast("error", "Chat error", errMsg);
            } else if (event.type === "done") {
              if (event.conversation_id) {
                setConversationId(event.conversation_id);
              }
              const ns = event.nexus_state;
              if (ns?.actions && Array.isArray(ns.actions)) {
                setProcessingSteps((prev) => {
                  const updated = [...prev];
                  for (const a of ns.actions as { action: string; payload: Record<string, unknown> }[]) {
                    const already = updated.find((s) => s.action === a.action);
                    if (!already) {
                      updated.push({ action: a.action, data: a.payload ?? {}, done: false });
                    }
                  }
                  return updated;
                });
              }
              setMessages((prev) =>
                prev.map((m) =>
                  m.id === assistantMsg.id
                    ? {
                        ...m,
                        streaming: false,
                        phase: ns?.phase,
                        phase_label: ns?.phase_label,
                        milestone: ns?.milestone ?? undefined,
                      }
                    : m
                )
              );
            }
          },
          abortRef.current.signal,
          chatProvider,
          chatModel,
        );
      } catch (e: unknown) {
        const errorMsg = e instanceof Error ? e.message : "Unknown error";
        const isAbort = e instanceof Error && e.name === "AbortError";
        if (!isAbort) {
          setMessages((prev) =>
            prev.map((m) =>
              m.id === assistantMsg.id
                ? { ...m, content: "Something went wrong. Please try again.", streaming: false }
                : m
            )
          );
          toast("error", "Chat failed", errorMsg);
        }
      } finally {
        setStreaming(false);
        setProcessingSteps([]);
        abortRef.current = null;
        currentAssistantId.current = null;
      }
    },
    [projectId, conversationId, streaming, chatProvider, chatModel, toast]
  );

  useEffect(() => {
    sendMessageRef.current = sendMessage;
  }, [sendMessage]);

  function handleKeyDown(e: React.KeyboardEvent) {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      sendMessage(input);
    }
  }

  function stopStream() {
    abortRef.current?.abort();
    setStreaming(false);
    setProcessingSteps([]);
    setMessages((prev) =>
      prev.map((m) =>
        m.streaming ? { ...m, streaming: false } : m
      )
    );
  }

  return (
    <div className="flex flex-col h-full">
      <style>{`
        @keyframes projectChatBounce {
          0%, 80%, 100% { transform: translateY(0); opacity: 0.4; }
          40% { transform: translateY(-4px); opacity: 1; }
        }
      `}</style>

      <div className="flex-1 overflow-y-auto scrollbar-thin px-6 py-6 max-w-3xl mx-auto w-full dot-grid">
        {messages.length === 0 && (
          <div className="flex flex-col items-center justify-center h-full">
            <div className="w-14 h-14 rounded-2xl bg-gradient-to-br from-glow-cyan/20 to-glow-blue/20 border border-glow-cyan/10 flex items-center justify-center mb-4">
              <Sparkles className="w-7 h-7 text-glow-cyan/60" />
            </div>
            <p className="text-slate-400 text-sm mb-6">{emptyChatMessage}</p>
            <div className="flex flex-wrap gap-2 max-w-lg justify-center">
              {[
                { label: "Build a CRM app", icon: "🏗️" },
                { label: "Create AI support agents", icon: "🤖" },
                { label: "Set up a content pipeline", icon: "🔄" },
                { label: "Design a lead qualification system", icon: "🎯" },
                { label: "Build an invoicing workflow", icon: "📄" },
                { label: "Launch a client portal", icon: "🌐" },
              ].map(({ label: suggestion, icon }) => (
                <button
                  key={suggestion}
                  onClick={() => {
                    setInput(suggestion);
                    sendMessage(suggestion);
                  }}
                  className="px-3 py-1.5 rounded-full border border-white/[0.08] bg-white/[0.02] hover:bg-white/[0.05] text-xs text-slate-400 hover:text-slate-200 transition-colors flex items-center gap-1.5"
                >
                  <span>{icon}</span>
                  {suggestion}
                </button>
              ))}
            </div>
          </div>
        )}
        {messages.map((msg) => (
          <MessageBubble
            key={msg.id}
            msg={msg}
            projectId={projectId}
            processingSteps={
              msg.streaming && msg.id === currentAssistantId.current
                ? processingSteps
                : undefined
            }
          />
        ))}
        <div ref={bottomRef} />
      </div>

      <div className="border-t border-white/[0.06] glass px-6 py-3">
        <div className="max-w-3xl mx-auto flex gap-2 items-center">
          <ModelSelector
            selectedProvider={chatProvider}
            selectedModel={chatModel}
            onChange={(provider, model) => {
              setChatProvider(provider);
              setChatModel(model);
              api.setProjectModel(projectId, provider, model).catch(() => {});
            }}
            className="flex-shrink-0"
          />
          <div className="flex-1 relative">
            <textarea
              value={input}
              onChange={(e) => setInput(e.target.value)}
              onKeyDown={handleKeyDown}
              placeholder={chatPlaceholder}
              rows={1}
              disabled={streaming}
              className={cn(
                "w-full px-4 py-2.5 rounded-xl text-[14px] resize-none",
                "bg-transparent border border-white/[0.08] text-slate-200",
                "placeholder:text-slate-400/50",
                "focus:outline-none focus:shadow-glow-sm focus:border-glow-cyan/40",
                "min-h-[42px] max-h-[140px] overflow-y-auto",
                streaming && "opacity-60 cursor-not-allowed"
              )}
              style={{ height: "auto" }}
              onInput={(e) => {
                const el = e.currentTarget;
                el.style.height = "auto";
                el.style.height = Math.min(el.scrollHeight, 140) + "px";
              }}
            />
          </div>
          {streaming ? (
            <Button
              variant="outline"
              size="icon"
              onClick={stopStream}
              className="flex-shrink-0 h-[42px] w-[42px] rounded-xl"
              aria-label="Stop response"
            >
              <Square className="w-4 h-4" />
            </Button>
          ) : (
            <Button
              size="icon"
              onClick={() => sendMessage(input)}
              disabled={!input.trim()}
              className="flex-shrink-0 h-[42px] w-[42px] rounded-xl"
              aria-label="Send message"
            >
              <Send className="w-4 h-4" />
            </Button>
          )}
        </div>
        <div className="max-w-3xl mx-auto mt-1.5 flex items-center justify-between">
          <span className="text-[10px] text-slate-400/40">
            Press <kbd className="px-1 py-0.5 rounded bg-white/[0.06] text-slate-400/50 font-mono text-[9px]">Enter</kbd> to send, <kbd className="px-1 py-0.5 rounded bg-white/[0.06] text-slate-400/50 font-mono text-[9px]">Shift+Enter</kbd> for new line
          </span>
          {streaming && (
            <span className="text-[10px] text-slate-400/40 flex items-center gap-1">
              <span className="w-1.5 h-1.5 rounded-full bg-glow-cyan animate-pulse" />
              Streaming...
            </span>
          )}
        </div>
      </div>
    </div>
  );
}
