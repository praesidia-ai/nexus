"use client";

import {
  Monitor,
  Server,
  Database,
  Shield,
  Cloud,
  CheckCircle2,
  AlertTriangle,
} from "lucide-react";
import { cn } from "@/lib/utils";
import { Card } from "@/components/ui/card";

export interface StackDecision {
  area: string;
  choice: string;
  confidence: number;
  coherent: boolean;
  learningOverride?: boolean;
}

const AREA_CONFIG: Record<string, { icon: React.ElementType; label: string; color: string }> = {
  frontend: { icon: Monitor, label: "Frontend", color: "text-sky-400" },
  backend: { icon: Server, label: "Backend", color: "text-violet-400" },
  database: { icon: Database, label: "Database", color: "text-emerald-400" },
  auth: { icon: Shield, label: "Auth", color: "text-amber-400" },
  hosting: { icon: Cloud, label: "Hosting", color: "text-pink-400" },
};

function confidenceColor(confidence: number): string {
  if (confidence >= 0.8) return "bg-emerald-500/20 border-emerald-500/30";
  if (confidence >= 0.5) return "bg-amber-500/20 border-amber-500/30";
  return "bg-red-500/20 border-red-500/30";
}

function confidenceTextColor(confidence: number): string {
  if (confidence >= 0.8) return "text-emerald-400";
  if (confidence >= 0.5) return "text-amber-400";
  return "text-red-400";
}

interface Props {
  decisions: StackDecision[];
  onSelect?: (area: string) => void;
  selectedArea?: string;
  className?: string;
}

export function StackDiagram({ decisions, onSelect, selectedArea, className }: Props) {
  if (decisions.length === 0) {
    return (
      <div className={cn("text-center py-8 text-sm text-slate-400", className)}>
        No architecture decisions yet
      </div>
    );
  }

  return (
    <div className={cn("space-y-2", className)}>
      {decisions.map((decision) => {
        const config = AREA_CONFIG[decision.area] ?? {
          icon: Server,
          label: decision.area,
          color: "text-slate-400",
        };
        const Icon = config.icon;
        const isSelected = selectedArea === decision.area;

        return (
          <Card
            key={decision.area}
            onClick={() => onSelect?.(decision.area)}
            className={cn(
              "p-3 cursor-pointer transition-all border",
              isSelected
                ? "border-glow-cyan/40 bg-glow-cyan/[0.05]"
                : "border-white/[0.08] hover:border-white/[0.15] hover:bg-white/[0.02]",
              confidenceColor(decision.confidence)
            )}
          >
            <div className="flex items-center gap-3">
              <div className={cn("w-8 h-8 rounded-lg flex items-center justify-center shrink-0", `${config.color}/10`.replace("text-", "bg-"))}>
                <Icon className={cn("h-4 w-4", config.color)} />
              </div>

              <div className="flex-1 min-w-0">
                <div className="flex items-center gap-2">
                  <span className="text-[11px] text-slate-400 uppercase tracking-wider font-medium">
                    {config.label}
                  </span>
                  {decision.learningOverride && (
                    <span className="text-[9px] bg-violet-500/20 text-violet-400 px-1.5 py-0.5 rounded-full font-medium">
                      learned
                    </span>
                  )}
                </div>
                <p className="text-[13px] font-semibold text-slate-200 mt-0.5 truncate">
                  {decision.choice}
                </p>
              </div>

              <div className="flex items-center gap-2 shrink-0">
                <span className={cn("text-[11px] font-medium tabular-nums", confidenceTextColor(decision.confidence))}>
                  {Math.round(decision.confidence * 100)}%
                </span>
                {decision.coherent ? (
                  <CheckCircle2 className="h-3.5 w-3.5 text-emerald-400" />
                ) : (
                  <AlertTriangle className="h-3.5 w-3.5 text-amber-400" />
                )}
              </div>
            </div>
          </Card>
        );
      })}
    </div>
  );
}
