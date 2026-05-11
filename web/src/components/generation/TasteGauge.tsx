"use client";

import { cn } from "@/lib/utils";

interface Props {
  score: number | null;
  size?: number;
  className?: string;
}

function scoreColor(score: number): string {
  if (score >= 70) return "text-emerald-400";
  if (score >= 50) return "text-amber-400";
  return "text-red-400";
}

function scoreStrokeColor(score: number): string {
  if (score >= 70) return "stroke-emerald-400";
  if (score >= 50) return "stroke-amber-400";
  return "stroke-red-400";
}

function scoreLabel(score: number): string {
  if (score >= 90) return "Excellent";
  if (score >= 70) return "Good";
  if (score >= 50) return "Fair";
  return "Needs work";
}

export function TasteGauge({ score, size = 120, className }: Props) {
  const radius = (size - 12) / 2;
  const circumference = 2 * Math.PI * radius;
  const normalizedScore = score != null ? Math.max(0, Math.min(100, score)) : 0;
  const offset = circumference - (normalizedScore / 100) * circumference;

  return (
    <div className={cn("flex flex-col items-center gap-2", className)}>
      <div className="relative" style={{ width: size, height: size }}>
        <svg
          width={size}
          height={size}
          viewBox={`0 0 ${size} ${size}`}
          className="transform -rotate-90"
        >
          {/* Background circle */}
          <circle
            cx={size / 2}
            cy={size / 2}
            r={radius}
            fill="none"
            stroke="currentColor"
            strokeWidth={6}
            className="text-white/[0.06]"
          />
          {/* Score arc */}
          {score != null && (
            <circle
              cx={size / 2}
              cy={size / 2}
              r={radius}
              fill="none"
              strokeWidth={6}
              strokeLinecap="round"
              strokeDasharray={circumference}
              strokeDashoffset={offset}
              className={cn(
                "transition-all duration-1000 ease-out",
                scoreStrokeColor(normalizedScore)
              )}
            />
          )}
        </svg>
        {/* Center text */}
        <div className="absolute inset-0 flex flex-col items-center justify-center">
          {score != null ? (
            <>
              <span className={cn("text-2xl font-bold tabular-nums", scoreColor(normalizedScore))}>
                {Math.round(normalizedScore)}
              </span>
              <span className="text-[10px] text-slate-400">/ 100</span>
            </>
          ) : (
            <span className="text-sm text-slate-400">--</span>
          )}
        </div>
      </div>
      {score != null && (
        <span className={cn("text-xs font-medium", scoreColor(normalizedScore))}>
          {scoreLabel(normalizedScore)}
        </span>
      )}
    </div>
  );
}
