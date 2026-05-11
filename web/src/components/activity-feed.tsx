"use client";

import { useState, useEffect } from "react";
import {
  Activity,
  Bot,
  Database,
  FileCode,
  MessageSquare,
  Zap,
} from "lucide-react";
import { cn } from "@/lib/utils";
import { BASE } from "@/lib/api";
import { ScrollArea } from "@/components/ui/scroll-area";

interface ActivityEvent {
  id: string;
  type: string;
  summary: string;
  timestamp: string;
  project_id?: string;
}

interface TraceRow {
  id?: string;
  agent_name?: string;
  action?: string;
  status?: string;
  created_at?: string;
  project_id?: string;
}

function traceToActivity(t: TraceRow, i: number): ActivityEvent {
  const action = t.action ?? "trace";
  const agent = t.agent_name ?? "";
  return {
    id: t.id ?? `${i}-${t.created_at ?? ""}`,
    type: agent ? "agent" : "generation",
    summary: agent ? `${agent}: ${action}` : action,
    timestamp: t.created_at ?? new Date().toISOString(),
    project_id: t.project_id,
  };
}

const ICON_MAP: Record<string, React.ElementType> = {
  agent: Bot,
  generation: Zap,
  database: Database,
  file: FileCode,
  chat: MessageSquare,
};

function timeAgo(timestamp: string): string {
  const diff = Date.now() - new Date(timestamp).getTime();
  const seconds = Math.floor(diff / 1000);
  if (seconds < 60) return `${seconds}s ago`;
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes}m ago`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours}h ago`;
  return `${Math.floor(hours / 24)}d ago`;
}

interface ActivityFeedProps {
  projectId?: string;
  className?: string;
  maxItems?: number;
}

export function ActivityFeed({
  projectId,
  className,
  maxItems = 20,
}: ActivityFeedProps) {
  const [events, setEvents] = useState<ActivityEvent[]>([]);

  useEffect(() => {
    let mounted = true;

    const fetchEvents = async () => {
      if (!projectId) {
        // Global activity feed has no backend route yet; stay empty.
        return;
      }
      try {
        const res = await fetch(
          `${BASE}/projects/${projectId}/traces?limit=${maxItems}`,
        );
        if (!res.ok) return;
        const data = (await res.json()) as TraceRow[] | { traces?: TraceRow[] };
        const rows = Array.isArray(data) ? data : data.traces ?? [];
        if (mounted) {
          setEvents(rows.map(traceToActivity));
        }
      } catch {
        // Activity feed is optional; silently ignore transient errors
      }
    };

    fetchEvents();
    const interval = setInterval(fetchEvents, 15000);
    return () => {
      mounted = false;
      clearInterval(interval);
    };
  }, [projectId, maxItems]);

  return (
    <div className={cn("flex flex-col", className)}>
      <div className="flex items-center gap-2 px-4 py-3 border-b border-white/[0.06]">
        <Activity className="w-3.5 h-3.5 text-slate-400" />
        <h3 className="text-xs font-semibold text-slate-200">
          Recent Activity
        </h3>
      </div>

      <ScrollArea className="flex-1">
        {events.length === 0 ? (
          <div className="px-4 py-8 text-center text-[11px] text-slate-400/50">
            No recent activity
          </div>
        ) : (
          <div className="py-1">
            {events.map((event) => {
              const Icon = ICON_MAP[event.type] || Activity;
              return (
                <div
                  key={event.id}
                  className="flex items-start gap-2.5 px-4 py-2 hover:bg-white/[0.02] transition-colors"
                >
                  <div className="w-6 h-6 rounded-md bg-white/[0.04] flex items-center justify-center flex-shrink-0 mt-0.5">
                    <Icon className="w-3 h-3 text-slate-400" />
                  </div>
                  <div className="flex-1 min-w-0">
                    <p className="text-[12px] text-slate-200 leading-snug line-clamp-2">
                      {event.summary}
                    </p>
                    <p className="text-[10px] text-slate-400/50 mt-0.5">
                      {timeAgo(event.timestamp)}
                    </p>
                  </div>
                </div>
              );
            })}
          </div>
        )}
      </ScrollArea>
    </div>
  );
}
