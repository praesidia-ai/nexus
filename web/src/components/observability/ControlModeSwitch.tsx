"use client";

import { useState, useEffect, useSyncExternalStore } from "react";
import {
  Shield,
  Eye,
  Zap,
  Lock,
  AlertTriangle,
  CheckCircle2,
} from "lucide-react";
import { cn } from "@/lib/utils";
import { api } from "@/lib/api";
import { controlStore, type ControlMode } from "@/stores/controlStore";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";

const MODE_CONFIG: Record<ControlMode, {
  icon: React.ElementType;
  label: string;
  description: string;
  color: string;
  bgColor: string;
  apiMode: string;
}> = {
  autonomous: {
    icon: Zap,
    label: "Autonomous",
    description: "Full auto -- Nexus acts without approval",
    color: "text-emerald-400",
    bgColor: "bg-emerald-500/10 border-emerald-500/30",
    apiMode: "autonomous",
  },
  supervised: {
    icon: Eye,
    label: "Supervised",
    description: "Nexus proposes, you approve critical actions",
    color: "text-sky-400",
    bgColor: "bg-sky-500/10 border-sky-500/30",
    apiMode: "assisted",
  },
  manual: {
    icon: Shield,
    label: "Manual",
    description: "You direct all actions, Nexus assists only",
    color: "text-amber-400",
    bgColor: "bg-amber-500/10 border-amber-500/30",
    apiMode: "safe",
  },
  locked: {
    icon: Lock,
    label: "Locked",
    description: "All operations paused -- read-only mode",
    color: "text-red-400",
    bgColor: "bg-red-500/10 border-red-500/30",
    apiMode: "safe",
  },
};

interface Props {
  className?: string;
}

export function ControlModeSwitch({ className }: Props) {
  const state = useSyncExternalStore(
    controlStore.subscribe,
    controlStore.getSnapshot,
    controlStore.getSnapshot
  );

  const [pendingMode, setPendingMode] = useState<ControlMode | null>(null);
  const [saving, setSaving] = useState(false);

  // Load initial state from API
  useEffect(() => {
    api.getControlStatus().then((status) => {
      controlStore.updateFromApi(status);
    }).catch(() => {});
  }, []);

  async function handleModeChange(mode: ControlMode) {
    if (mode === state.mode) return;

    // For dangerous mode changes, require confirmation
    if (mode === "autonomous" || mode === "locked") {
      setPendingMode(mode);
      return;
    }

    await applyMode(mode);
  }

  async function applyMode(mode: ControlMode) {
    setSaving(true);
    setPendingMode(null);

    try {
      const config = MODE_CONFIG[mode];
      const apiMode = config.apiMode as "safe" | "assisted" | "autonomous";
      const result = await api.setControlMode(apiMode);
      controlStore.updateFromApi(result as unknown as { mode: string; limits: unknown; capabilities: Record<string, string> });
      // Also set local mode since API mode names differ
      controlStore.setMode(mode);
    } catch {
      // Revert on error
    } finally {
      setSaving(false);
    }
  }

  return (
    <div className={cn("space-y-4", className)}>
      {/* Mode selector */}
      <div className="grid grid-cols-2 gap-2">
        {(Object.entries(MODE_CONFIG) as [ControlMode, typeof MODE_CONFIG[ControlMode]][]).map(
          ([mode, config]) => {
            const Icon = config.icon;
            const isActive = state.mode === mode;

            return (
              <button
                key={mode}
                onClick={() => handleModeChange(mode)}
                disabled={saving}
                className={cn(
                  "flex items-start gap-3 p-3 rounded-lg border text-left transition-all",
                  isActive
                    ? config.bgColor
                    : "border-white/[0.08] bg-white/[0.02] hover:bg-white/[0.04] hover:border-white/[0.12]",
                  saving && "opacity-50 cursor-not-allowed"
                )}
              >
                <Icon className={cn("h-4 w-4 mt-0.5 shrink-0", isActive ? config.color : "text-slate-400")} />
                <div>
                  <p className={cn("text-[12px] font-semibold", isActive ? config.color : "text-slate-200")}>
                    {config.label}
                  </p>
                  <p className="text-[10px] text-slate-400 mt-0.5 leading-snug">
                    {config.description}
                  </p>
                </div>
              </button>
            );
          }
        )}
      </div>

      {/* Confirmation dialog */}
      {pendingMode && (
        <Card className="border-amber-500/30 bg-amber-500/[0.05]">
          <CardContent className="p-4 pt-4">
            <div className="flex items-start gap-2.5">
              <AlertTriangle className="h-4 w-4 text-amber-400 shrink-0 mt-0.5" />
              <div className="flex-1">
                <p className="text-sm font-medium text-amber-400">
                  Switch to {MODE_CONFIG[pendingMode].label} mode?
                </p>
                <p className="text-xs text-slate-400 mt-1">
                  {pendingMode === "autonomous"
                    ? "Nexus will take actions without asking for approval. Make sure you trust the current project state."
                    : "All operations will be paused. Active generation and agent processes will be stopped."}
                </p>
                <div className="flex items-center gap-2 mt-3">
                  <button
                    onClick={() => applyMode(pendingMode)}
                    className="px-3 py-1.5 rounded-lg text-xs font-medium bg-amber-500/20 text-amber-400 hover:bg-amber-500/30 transition-colors"
                  >
                    Confirm
                  </button>
                  <button
                    onClick={() => setPendingMode(null)}
                    className="px-3 py-1.5 rounded-lg text-xs font-medium text-slate-400 hover:text-slate-200 transition-colors"
                  >
                    Cancel
                  </button>
                </div>
              </div>
            </div>
          </CardContent>
        </Card>
      )}

      {/* Circuit breakers */}
      {state.circuitBreakers.length > 0 && (
        <Card>
          <CardHeader className="pb-2 pt-3 px-4 border-b border-white/[0.06]">
            <CardTitle className="text-sm flex items-center gap-2">
              Circuit breakers
              <Badge variant={state.circuitBreakers.some((b) => b.triggered) ? "destructive" : "success"} className="text-[9px]">
                {state.circuitBreakers.filter((b) => b.triggered).length} triggered
              </Badge>
            </CardTitle>
          </CardHeader>
          <CardContent className="pt-3 px-4 pb-4">
            <div className="space-y-2">
              {state.circuitBreakers.map((breaker) => (
                <div
                  key={breaker.id}
                  className="flex items-center gap-2.5 text-xs"
                >
                  {breaker.triggered ? (
                    <AlertTriangle className="h-3.5 w-3.5 text-red-400 shrink-0" />
                  ) : (
                    <CheckCircle2 className="h-3.5 w-3.5 text-emerald-400 shrink-0" />
                  )}
                  <div className="flex-1 min-w-0">
                    <span className="font-medium text-slate-200">{breaker.name}</span>
                    <span className="text-slate-400 ml-1.5">{breaker.description}</span>
                  </div>
                </div>
              ))}
            </div>
          </CardContent>
        </Card>
      )}

      {/* Capabilities */}
      {Object.keys(state.capabilities).length > 0 && (
        <div className="space-y-1.5">
          <p className="text-[11px] text-slate-400 uppercase tracking-wider font-medium">
            Active capabilities
          </p>
          <div className="flex flex-wrap gap-1.5">
            {Object.entries(state.capabilities).map(([key, value]) => (
              <Badge
                key={key}
                variant={value === "enabled" ? "success" : value === "approval_required" ? "warning" : "secondary"}
                className="text-[10px]"
              >
                {key}: {value}
              </Badge>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}
