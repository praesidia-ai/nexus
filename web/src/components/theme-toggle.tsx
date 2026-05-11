"use client";

import { Moon, Monitor } from "lucide-react";
import { useTheme } from "./theme-provider";
import { cn } from "@/lib/utils";

export function ThemeToggle({ className }: { className?: string }) {
  const { theme, setTheme } = useTheme();
  const nextTheme = theme === "dark" ? "system" : "dark";
  const Icon = theme === "dark" ? Moon : Monitor;
  const label = theme === "dark" ? "Dark theme" : "System theme";

  return (
    <button
      onClick={() => setTheme(nextTheme)}
      className={cn(
        "p-2 rounded-lg transition-colors",
        "text-slate-400 hover:text-slate-200 hover:bg-white/[0.06]",
        "focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-glow-cyan/40",
        className,
      )}
      title={`${label}. Click to switch to ${nextTheme}.`}
      aria-label={label}
    >
      <Icon className="w-4 h-4" />
    </button>
  );
}
