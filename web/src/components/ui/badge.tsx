import * as React from "react";
import { cva, type VariantProps } from "class-variance-authority";
import { cn } from "@/lib/utils";

const badgeVariants = cva(
  "inline-flex items-center rounded-full border px-2.5 py-0.5 text-xs font-medium transition-colors focus:outline-none focus:shadow-glow-sm",
  {
    variants: {
      variant: {
        default: "border-transparent bg-glow-cyan/[0.12] text-glow-cyan",
        secondary: "border-transparent bg-white/5 text-slate-400",
        destructive: "border-transparent bg-destructive/15 text-destructive",
        outline: "border-white/10 text-slate-400",
        success: "border-transparent bg-emerald-500/15 text-emerald-400",
        warning: "border-transparent bg-amber-500/15 text-amber-400",
        info: "border-transparent bg-blue-500/15 text-blue-400",
        purple: "border-transparent bg-purple-500/15 text-purple-400",
      },
    },
    defaultVariants: {
      variant: "default",
    },
  }
);

export interface BadgeProps
  extends React.HTMLAttributes<HTMLDivElement>,
    VariantProps<typeof badgeVariants> {}

function Badge({ className, variant, ...props }: BadgeProps) {
  return (
    <div className={cn(badgeVariants({ variant }), className)} {...props} />
  );
}

export { Badge, badgeVariants };
