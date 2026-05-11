'use client';

import { useState, useEffect } from 'react';
import Link from 'next/link';
import { Coins, ExternalLink } from 'lucide-react';
import { cn } from '@/lib/utils';
import { api } from '@/lib/api';
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from '@/components/ui/tooltip';

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface CreditData {
  credits_total: number;
  credits_used: number;
  credits_remaining: number;
  plan: string;
  resets_at: string;
}

// ---------------------------------------------------------------------------
// CreditDisplay
// ---------------------------------------------------------------------------

export function CreditDisplay({ className }: { className?: string }) {
  const [credits, setCredits] = useState<CreditData | null>(null);

  useEffect(() => {
    let mounted = true;

    const fetchCredits = async () => {
      try {
        const data = await api.getCredits();
        if (mounted) setCredits(data);
      } catch {
        // Credits endpoint may not be available yet — use fallback
        if (mounted) {
          setCredits({
            credits_total: 50,
            credits_used: 0,
            credits_remaining: 50,
            plan: 'Free',
            resets_at: new Date(Date.now() + 30 * 86400000).toISOString(),
          });
        }
      }
    };

    fetchCredits();
    const interval = setInterval(fetchCredits, 30000);
    return () => {
      mounted = false;
      clearInterval(interval);
    };
  }, []);

  if (!credits) return null;

  const percentage = credits.credits_total > 0
    ? (credits.credits_remaining / credits.credits_total) * 100
    : 0;

  const barColor =
    percentage > 50
      ? 'bg-emerald-400'
      : percentage > 20
        ? 'bg-amber-400'
        : 'bg-red-400';

  const textColor =
    percentage > 50
      ? 'text-emerald-400'
      : percentage > 20
        ? 'text-amber-400'
        : 'text-red-400';

  const resetsAt = credits.resets_at
    ? new Date(credits.resets_at).toLocaleDateString('en-US', { month: 'short', day: 'numeric' })
    : 'N/A';

  return (
    <TooltipProvider delayDuration={300}>
      <Tooltip>
        <TooltipTrigger asChild>
          <div
            className={cn(
              'flex flex-col gap-1.5 px-2 py-2 rounded-lg',
              'border border-white/[0.06] bg-white/[0.02]',
              'cursor-default select-none',
              className,
            )}
          >
            {/* Top row: icon + balance */}
            <div className="flex items-center gap-1.5">
              <Coins className={cn('w-3.5 h-3.5 flex-shrink-0', textColor)} />
              <span className={cn('text-xs font-medium tabular-nums', textColor)}>
                {credits.credits_remaining}
              </span>
              <span className="text-[10px] text-slate-500">
                / {credits.credits_total} credits
              </span>
            </div>

            {/* Progress bar */}
            <div className="w-full h-1 rounded-full bg-white/[0.06] overflow-hidden">
              <div
                className={cn('h-full rounded-full transition-all duration-500', barColor)}
                style={{ width: `${Math.max(percentage, 2)}%` }}
              />
            </div>

            {/* Low credit warning */}
            {percentage <= 20 && (
              <Link
                href="/settings#billing"
                className="flex items-center gap-1 text-[10px] text-red-400 hover:text-red-300 transition-colors"
              >
                <ExternalLink className="w-2.5 h-2.5" />
                Get more credits
              </Link>
            )}
          </div>
        </TooltipTrigger>

        <TooltipContent side="right" className="text-xs">
          <div className="flex flex-col gap-0.5">
            <span>Plan: <strong>{credits.plan}</strong></span>
            <span>Credits: <strong>{credits.credits_remaining}</strong> / {credits.credits_total}</span>
            <span>Resets: <strong>{resetsAt}</strong></span>
          </div>
        </TooltipContent>
      </Tooltip>
    </TooltipProvider>
  );
}
