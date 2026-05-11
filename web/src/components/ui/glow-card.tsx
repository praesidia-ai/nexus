'use client';

import { forwardRef } from 'react';
import { cn } from '@/lib/utils';

type GlowVariant = 'cyan' | 'purple' | 'amber' | 'none';

interface GlowCardProps extends React.HTMLAttributes<HTMLDivElement> {
  glow?: GlowVariant;
  active?: boolean;
}

const glowBorderColors: Record<GlowVariant, string> = {
  cyan: 'hover:border-glow-cyan/20 hover:shadow-glow-sm',
  purple: 'hover:border-glow-purple/20 hover:shadow-glow-purple',
  amber: 'hover:border-glow-amber/20 hover:shadow-glow-amber',
  none: 'hover:border-white/10',
};

const activeStyles: Record<GlowVariant, string> = {
  cyan: 'border-glow-cyan/30 shadow-glow-sm',
  purple: 'border-glow-purple/30 shadow-glow-purple',
  amber: 'border-glow-amber/30 shadow-glow-amber',
  none: 'border-white/10',
};

export const GlowCard = forwardRef<HTMLDivElement, GlowCardProps>(
  ({ glow = 'cyan', active = false, className, children, ...props }, ref) => {
    return (
      <div
        ref={ref}
        className={cn(
          'relative rounded-nexus border border-white/[0.06] bg-white/[0.03] backdrop-blur-xl',
          'transition-all duration-300 ease-out',
          glowBorderColors[glow],
          active && activeStyles[glow],
          className
        )}
        {...props}
      >
        {/* Active state: top gradient line */}
        {active && glow !== 'none' && (
          <div
            className={cn(
              'absolute top-0 left-4 right-4 h-px',
              glow === 'cyan' && 'bg-gradient-to-r from-transparent via-glow-cyan/50 to-transparent',
              glow === 'purple' && 'bg-gradient-to-r from-transparent via-glow-purple/50 to-transparent',
              glow === 'amber' && 'bg-gradient-to-r from-transparent via-glow-amber/50 to-transparent',
            )}
          />
        )}
        {children}
      </div>
    );
  }
);

GlowCard.displayName = 'GlowCard';
