'use client';

import { cn } from '@/lib/utils';

type StatusDotVariant = 'active' | 'idle' | 'error' | 'waiting';

interface StatusDotProps {
  status: StatusDotVariant;
  size?: 'sm' | 'md' | 'lg';
  className?: string;
  label?: string;
}

const dotColors: Record<StatusDotVariant, string> = {
  active: 'bg-glow-cyan',
  idle: 'bg-slate-500',
  error: 'bg-red-500',
  waiting: 'bg-glow-amber',
};

const dotGlow: Record<StatusDotVariant, string> = {
  active: 'shadow-[0_0_6px_rgba(0,212,255,0.6)] animate-glow-breathe text-glow-cyan',
  idle: '',
  error: 'shadow-[0_0_6px_rgba(239,68,68,0.6)] animate-glow-breathe text-red-500',
  waiting: 'shadow-[0_0_6px_rgba(245,158,11,0.5)] animate-pulse-glow text-glow-amber',
};

const sizeClasses: Record<'sm' | 'md' | 'lg', string> = {
  sm: 'h-1.5 w-1.5',
  md: 'h-2.5 w-2.5',
  lg: 'h-3.5 w-3.5',
};

export function StatusDot({ status, size = 'md', className, label }: StatusDotProps) {
  return (
    <span
      className={cn('inline-flex items-center gap-2', className)}
      role="status"
      aria-label={label ?? `Status: ${status}`}
    >
      <span
        className={cn(
          'rounded-full',
          dotColors[status],
          dotGlow[status],
          sizeClasses[size],
        )}
      />
      {label && (
        <span className="text-xs text-[var(--text-secondary)]">{label}</span>
      )}
    </span>
  );
}
