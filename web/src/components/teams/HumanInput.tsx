"use client";

import { useState } from "react";
import {
  Send,
  Pause,
  Play,
  ChevronDown,
} from "lucide-react";
import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import { useToast } from "@/components/toast";
import type { MemberStatus } from "@/hooks/useTeamExecution";

interface Props {
  members: MemberStatus[];
  status: "idle" | "running" | "paused" | "completed" | "failed";
  onInject: (message: string, target?: string) => Promise<void>;
  onPause: () => Promise<{ ok: boolean; error?: string } | void>;
  onResume: () => Promise<{ ok: boolean; error?: string } | void>;
}

export function HumanInput({ members, status, onInject, onPause, onResume }: Props) {
  const { toast } = useToast();
  const [message, setMessage] = useState("");
  const [target, setTarget] = useState<string>("all");
  const [showTargets, setShowTargets] = useState(false);
  const [sending, setSending] = useState(false);

  const handlePause = async () => {
    const r = await onPause();
    if (r && typeof r === "object" && "ok" in r && !r.ok) {
      toast("error", "Could not pause", r.error ?? "Unknown error");
    }
  };
  const handleResume = async () => {
    const r = await onResume();
    if (r && typeof r === "object" && "ok" in r && !r.ok) {
      toast("error", "Could not resume", r.error ?? "Unknown error");
    }
  };

  const canInteract = status === "running" || status === "paused";

  const handleSend = async () => {
    if (!message.trim() || sending) return;
    setSending(true);
    try {
      await onInject(message.trim(), target === "all" ? undefined : target);
      setMessage("");
    } finally {
      setSending(false);
    }
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      handleSend();
    }
  };

  return (
    <div className="border-t border-white/[0.06] bg-white/[0.01] px-4 py-3">
      <div className="flex items-end gap-2">
        {/* Target selector */}
        <div className="relative">
          <button
            onClick={() => setShowTargets(!showTargets)}
            className="flex items-center gap-1.5 px-2.5 py-2 rounded-lg border border-white/[0.08] bg-white/[0.03] text-xs text-slate-400 hover:bg-white/[0.05] transition-colors"
          >
            <span className="min-w-[40px] text-left">
              {target === "all" ? "All" : target}
            </span>
            <ChevronDown className="w-3 h-3" />
          </button>
          {showTargets && (
            <div className="absolute bottom-full mb-1 left-0 min-w-[140px] rounded-lg border border-white/[0.1] bg-[#111118] shadow-2xl z-10 py-1">
              <button
                onClick={() => {
                  setTarget("all");
                  setShowTargets(false);
                }}
                className={cn(
                  "w-full text-left px-3 py-1.5 text-xs transition-colors",
                  target === "all"
                    ? "bg-glow-cyan/[0.08] text-glow-cyan"
                    : "text-slate-400 hover:bg-white/[0.05] hover:text-slate-200"
                )}
              >
                All Members
              </button>
              {members.map((m) => (
                <button
                  key={m.id}
                  onClick={() => {
                    setTarget(m.name);
                    setShowTargets(false);
                  }}
                  className={cn(
                    "w-full text-left px-3 py-1.5 text-xs transition-colors flex items-center gap-2",
                    target === m.name
                      ? "bg-glow-cyan/[0.08] text-glow-cyan"
                      : "text-slate-400 hover:bg-white/[0.05] hover:text-slate-200"
                  )}
                >
                  <span className="w-4 h-4 rounded bg-white/[0.06] flex items-center justify-center text-[9px] font-semibold">
                    {m.name[0]}
                  </span>
                  {m.name}
                </button>
              ))}
            </div>
          )}
        </div>

        {/* Text input */}
        <div className="flex-1 relative">
          <textarea
            value={message}
            onChange={(e) => setMessage(e.target.value)}
            onKeyDown={handleKeyDown}
            placeholder={canInteract ? "Inject a message to the team..." : "Team is not running"}
            disabled={!canInteract}
            rows={1}
            className="w-full text-sm bg-white/[0.03] border border-white/[0.08] rounded-lg px-3 py-2 text-slate-200 placeholder:text-slate-400/40 focus:outline-none focus:border-white/20 resize-none disabled:opacity-40"
          />
        </div>

        {/* Send button */}
        <Button
          onClick={handleSend}
          disabled={!canInteract || !message.trim() || sending}
          size="sm"
          className="bg-glow-cyan/[0.08] text-glow-cyan hover:bg-glow-cyan/[0.12] border border-glow-cyan/20"
        >
          <Send className="w-3.5 h-3.5" />
        </Button>

        {/* Control buttons */}
        <div className="flex items-center gap-1.5 ml-2">
          {status === "running" && (
            <Button
              onClick={handlePause}
              size="sm"
              variant="ghost"
              className="text-amber-400 hover:bg-amber-500/10"
            >
              <Pause className="w-3.5 h-3.5 mr-1" />
              Pause
            </Button>
          )}
          {status === "paused" && (
            <Button
              onClick={handleResume}
              size="sm"
              variant="ghost"
              className="text-emerald-400 hover:bg-emerald-500/10"
            >
              <Play className="w-3.5 h-3.5 mr-1" />
              Resume
            </Button>
          )}
        </div>
      </div>
    </div>
  );
}
