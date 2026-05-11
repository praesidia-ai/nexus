'use client';

import { Zap, Scale, Sparkles } from 'lucide-react';
import { cn } from '@/lib/utils';

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export type QualityLevel = 'fast' | 'balanced' | 'best';

interface QualityOption {
  value: QualityLevel;
  label: string;
  description: string;
  icon: typeof Zap;
  color: string;
  activeColor: string;
  activeBg: string;
  credits: string;
}

const OPTIONS: QualityOption[] = [
  {
    value: 'fast',
    label: 'Fast',
    description: 'Quick responses, lower cost',
    icon: Zap,
    color: 'text-emerald-400',
    activeColor: 'text-emerald-300',
    activeBg: 'bg-emerald-500/[0.12] border-emerald-500/30',
    credits: '~1 credit',
  },
  {
    value: 'balanced',
    label: 'Balanced',
    description: 'Best mix of speed and quality',
    icon: Scale,
    color: 'text-glow-cyan',
    activeColor: 'text-glow-cyan',
    activeBg: 'bg-glow-cyan/[0.12] border-glow-cyan/30',
    credits: '~2 credits',
  },
  {
    value: 'best',
    label: 'Best',
    description: 'Highest quality, more credits',
    icon: Sparkles,
    color: 'text-purple-400',
    activeColor: 'text-purple-300',
    activeBg: 'bg-purple-500/[0.12] border-purple-500/30',
    credits: '~5 credits',
  },
];

// ---------------------------------------------------------------------------
// QualitySelector
// ---------------------------------------------------------------------------

interface QualitySelectorProps {
  value: QualityLevel;
  onChange: (quality: QualityLevel) => void;
  className?: string;
}

export function QualitySelector({ value, onChange, className }: QualitySelectorProps) {
  return (
    <div className={cn('flex items-center gap-1 p-0.5 rounded-lg border border-white/[0.08] bg-white/[0.02]', className)}>
      {OPTIONS.map((opt) => {
        const isActive = value === opt.value;
        const Icon = opt.icon;

        return (
          <button
            key={opt.value}
            type="button"
            onClick={() => onChange(opt.value)}
            title={opt.description}
            className={cn(
              'relative flex items-center gap-1.5 px-2.5 py-1.5 rounded-md text-xs transition-all duration-200',
              'focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-glow-cyan/40',
              isActive
                ? cn('border', opt.activeBg, opt.activeColor, 'font-medium shadow-sm')
                : 'border border-transparent text-slate-400 hover:text-slate-200 hover:bg-white/[0.04]',
            )}
          >
            <Icon className="w-3 h-3 flex-shrink-0" />
            <span>{opt.label}</span>
            <span
              className={cn(
                'ml-0.5 px-1.5 py-0.5 rounded-full text-[9px] font-medium leading-none',
                isActive
                  ? 'bg-white/[0.08] text-inherit'
                  : 'bg-white/[0.04] text-slate-500',
              )}
            >
              {opt.credits}
            </span>
          </button>
        );
      })}
    </div>
  );
}
