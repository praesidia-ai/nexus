"use client";

import { useRef, useEffect, useState } from "react";
import { cn } from "@/lib/utils";
import {
  MessageSquare,
  FileText,
  AlertTriangle,
  Vote,
  ArrowDown,
  CheckCircle2,
  Info,
  Send,
} from "lucide-react";
import type { TeamMessage } from "@/hooks/useTeamExecution";

interface Props {
  messages: TeamMessage[];
}

// ---------------------------------------------------------------------------
// Message type visuals
// ---------------------------------------------------------------------------

const MSG_TYPE_CONFIG: Record<
  TeamMessage["type"],
  { icon: typeof MessageSquare; color: string; bgColor: string }
> = {
  task: { icon: FileText, color: "text-blue-400", bgColor: "bg-blue-500/10" },
  review: { icon: CheckCircle2, color: "text-emerald-400", bgColor: "bg-emerald-500/10" },
  proposal: { icon: Send, color: "text-purple-400", bgColor: "bg-purple-500/10" },
  vote: { icon: Vote, color: "text-amber-400", bgColor: "bg-amber-500/10" },
  escalation: { icon: AlertTriangle, color: "text-red-400", bgColor: "bg-red-500/10" },
  info: { icon: Info, color: "text-slate-400", bgColor: "bg-white/[0.04]" },
  result: { icon: CheckCircle2, color: "text-emerald-400", bgColor: "bg-emerald-500/10" },
};

// Color palette for different senders
const SENDER_COLORS: string[] = [
  "text-blue-400",
  "text-emerald-400",
  "text-purple-400",
  "text-amber-400",
  "text-rose-400",
  "text-cyan-400",
  "text-orange-400",
  "text-indigo-400",
];

function getSenderColor(name: string, senderMap: Map<string, string>): string {
  if (!senderMap.has(name)) {
    senderMap.set(name, SENDER_COLORS[senderMap.size % SENDER_COLORS.length]);
  }
  return senderMap.get(name)!;
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export function CommunicationFeed({ messages }: Props) {
  const containerRef = useRef<HTMLDivElement>(null);
  const [autoScroll, setAutoScroll] = useState(true);
  const [newCount, setNewCount] = useState(0);
  const senderMapRef = useRef(new Map<string, string>());
  const prevLengthRef = useRef(messages.length);

  // Auto-scroll behavior
  useEffect(() => {
    if (autoScroll && containerRef.current) {
      containerRef.current.scrollTop = containerRef.current.scrollHeight;
      setNewCount(0);
    } else if (messages.length > prevLengthRef.current) {
      setNewCount((prev) => prev + (messages.length - prevLengthRef.current));
    }
    prevLengthRef.current = messages.length;
  }, [messages.length, autoScroll]);

  const handleScroll = () => {
    if (!containerRef.current) return;
    const { scrollTop, scrollHeight, clientHeight } = containerRef.current;
    const isNearBottom = scrollHeight - scrollTop - clientHeight < 80;
    setAutoScroll(isNearBottom);
    if (isNearBottom) setNewCount(0);
  };

  const scrollToBottom = () => {
    if (containerRef.current) {
      containerRef.current.scrollTop = containerRef.current.scrollHeight;
    }
    setAutoScroll(true);
    setNewCount(0);
  };

  if (messages.length === 0) {
    return (
      <div className="flex flex-col items-center justify-center h-full text-slate-400/50">
        <MessageSquare className="w-8 h-8 mb-2" />
        <span className="text-sm">No messages yet</span>
        <span className="text-xs mt-1">Communication will appear here as the team works</span>
      </div>
    );
  }

  return (
    <div className="relative h-full flex flex-col">
      <div className="px-3 py-2 border-b border-white/[0.06]">
        <h3 className="text-[11px] text-slate-400 uppercase tracking-wider">
          Communication Feed
        </h3>
      </div>

      <div
        ref={containerRef}
        onScroll={handleScroll}
        className="flex-1 overflow-y-auto scrollbar-thin px-3 py-2 space-y-1.5"
      >
        {messages.map((msg) => {
          const typeConfig = MSG_TYPE_CONFIG[msg.type] ?? MSG_TYPE_CONFIG.info;
          const TypeIcon = typeConfig.icon;
          const senderColor = getSenderColor(msg.from, senderMapRef.current);

          return (
            <div
              key={msg.id}
              className="group flex items-start gap-2 py-1.5 px-2 rounded-lg hover:bg-white/[0.02] transition-colors"
            >
              <div className={cn("w-5 h-5 rounded flex items-center justify-center flex-shrink-0 mt-0.5", typeConfig.bgColor)}>
                <TypeIcon className={cn("w-3 h-3", typeConfig.color)} />
              </div>
              <div className="flex-1 min-w-0">
                <div className="flex items-baseline gap-1.5">
                  <span className={cn("text-[12px] font-semibold", senderColor)}>
                    {msg.from}
                  </span>
                  {msg.to !== "all" && (
                    <>
                      <span className="text-[10px] text-slate-400/40">to</span>
                      <span className="text-[11px] text-slate-400">{msg.to}</span>
                    </>
                  )}
                  <span className="text-[10px] text-slate-400/30 ml-auto flex-shrink-0 opacity-0 group-hover:opacity-100 transition-opacity">
                    {new Date(msg.timestamp).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", second: "2-digit" })}
                  </span>
                </div>
                <p className="text-[12px] text-slate-200/80 whitespace-pre-wrap break-words leading-relaxed mt-0.5">
                  {msg.content}
                </p>
              </div>
            </div>
          );
        })}
      </div>

      {/* New messages indicator */}
      {!autoScroll && newCount > 0 && (
        <button
          onClick={scrollToBottom}
          className="absolute bottom-3 left-1/2 -translate-x-1/2 flex items-center gap-1.5 px-3 py-1.5 rounded-full bg-glow-cyan/90 text-white text-xs font-medium shadow-lg hover:bg-glow-cyan transition-colors"
        >
          <ArrowDown className="w-3 h-3" />
          {newCount} new message{newCount > 1 ? "s" : ""}
        </button>
      )}
    </div>
  );
}
