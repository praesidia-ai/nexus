"use client";

import { Star } from "lucide-react";
import { cn } from "@/lib/utils";

interface Props {
  value: number;
  max?: number;
  onChange?: (value: number) => void;
  readonly?: boolean;
  size?: "sm" | "md";
}

export function StarRating({ value, max = 5, onChange, readonly = false, size = "md" }: Props) {
  const dim = size === "sm" ? "h-3 w-3" : "h-4 w-4";

  return (
    <div className="flex items-center gap-0.5">
      {Array.from({ length: max }).map((_, i) => {
        const filled = i < Math.round(value);
        return (
          <button
            key={i}
            type="button"
            disabled={readonly}
            onClick={() => onChange?.(i + 1)}
            className={cn(
              "transition-transform",
              !readonly && "hover:scale-110 cursor-pointer",
              readonly && "cursor-default",
            )}
          >
            <Star
              className={cn(
                dim,
                filled ? "fill-amber-400 text-amber-400" : "fill-transparent text-white/20",
              )}
            />
          </button>
        );
      })}
    </div>
  );
}
