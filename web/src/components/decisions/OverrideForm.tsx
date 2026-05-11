"use client";

import { useState } from "react";
import { AlertTriangle, ArrowRight } from "lucide-react";
import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";

export interface OverrideOption {
  area: string;
  choices: string[];
  currentChoice: string;
}

interface Props {
  options: OverrideOption[];
  coherenceWarnings?: string[];
  onOverride: (area: string, newChoice: string, reason: string) => void;
  className?: string;
}

export function OverrideForm({ options, coherenceWarnings, onOverride, className }: Props) {
  const [selectedArea, setSelectedArea] = useState<string>(options[0]?.area ?? "");
  const [selectedChoice, setSelectedChoice] = useState<string>("");
  const [reason, setReason] = useState("");
  const [showWarnings, setShowWarnings] = useState(false);

  const currentOption = options.find((o) => o.area === selectedArea);
  const availableChoices = currentOption?.choices.filter((c) => c !== currentOption.currentChoice) ?? [];

  const canSubmit = selectedArea && selectedChoice && reason.trim().length > 0;

  function handleSubmit() {
    if (!canSubmit) return;

    // Show coherence warnings first if any
    if (coherenceWarnings && coherenceWarnings.length > 0 && !showWarnings) {
      setShowWarnings(true);
      return;
    }

    onOverride(selectedArea, selectedChoice, reason);
    setSelectedChoice("");
    setReason("");
    setShowWarnings(false);
  }

  return (
    <div className={cn("space-y-4", className)}>
      {/* Area selector */}
      <div>
        <label className="text-[11px] text-slate-400 uppercase tracking-wider font-medium block mb-1.5">
          Decision area
        </label>
        <div className="flex flex-wrap gap-1.5">
          {options.map((opt) => (
            <button
              key={opt.area}
              onClick={() => {
                setSelectedArea(opt.area);
                setSelectedChoice("");
                setShowWarnings(false);
              }}
              className={cn(
                "px-3 py-1.5 rounded-lg text-[12px] font-medium border transition-colors capitalize",
                selectedArea === opt.area
                  ? "bg-glow-cyan/[0.08] text-glow-cyan border-glow-cyan/30"
                  : "bg-white/[0.03] text-slate-400 border-white/[0.08] hover:border-white/[0.15]"
              )}
            >
              {opt.area}
            </button>
          ))}
        </div>
      </div>

      {/* Current choice */}
      {currentOption && (
        <div className="flex items-center gap-2 text-xs text-slate-400">
          <span>Current:</span>
          <span className="font-medium text-slate-200">{currentOption.currentChoice}</span>
          {selectedChoice && (
            <>
              <ArrowRight className="h-3 w-3" />
              <span className="font-medium text-glow-cyan">{selectedChoice}</span>
            </>
          )}
        </div>
      )}

      {/* New choice */}
      <div>
        <label className="text-[11px] text-slate-400 uppercase tracking-wider font-medium block mb-1.5">
          New choice
        </label>
        <div className="flex flex-wrap gap-1.5">
          {availableChoices.map((choice) => (
            <button
              key={choice}
              onClick={() => {
                setSelectedChoice(choice);
                setShowWarnings(false);
              }}
              className={cn(
                "px-3 py-1.5 rounded-lg text-[12px] font-medium border transition-colors",
                selectedChoice === choice
                  ? "bg-glow-cyan/[0.08] text-glow-cyan border-glow-cyan/30"
                  : "bg-white/[0.03] text-slate-400 border-white/[0.08] hover:border-white/[0.15]"
              )}
            >
              {choice}
            </button>
          ))}
        </div>
      </div>

      {/* Reason */}
      <div>
        <label className="text-[11px] text-slate-400 uppercase tracking-wider font-medium block mb-1.5">
          Reason for override
        </label>
        <textarea
          value={reason}
          onChange={(e) => setReason(e.target.value)}
          placeholder="Why are you overriding this decision?"
          rows={3}
          className="w-full rounded-lg bg-white/[0.03] border border-white/[0.08] px-3 py-2 text-sm text-slate-200 placeholder:text-slate-400/40 focus:outline-none focus:border-glow-cyan/40 focus:shadow-glow-sm resize-none"
        />
      </div>

      {/* Coherence warnings */}
      {showWarnings && coherenceWarnings && coherenceWarnings.length > 0 && (
        <Card className="p-3 border-amber-500/30 bg-amber-500/[0.05]">
          <div className="flex items-start gap-2">
            <AlertTriangle className="h-4 w-4 text-amber-400 shrink-0 mt-0.5" />
            <div>
              <p className="text-xs font-medium text-amber-400 mb-1.5">Coherence warnings</p>
              <ul className="space-y-1">
                {coherenceWarnings.map((warning, i) => (
                  <li key={i} className="text-[11px] text-slate-400">
                    {warning}
                  </li>
                ))}
              </ul>
            </div>
          </div>
        </Card>
      )}

      {/* Submit */}
      <Button
        size="sm"
        onClick={handleSubmit}
        disabled={!canSubmit}
        className="w-full"
      >
        {showWarnings ? "Confirm override despite warnings" : "Override decision"}
      </Button>
    </div>
  );
}
