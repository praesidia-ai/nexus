'use client';

import { cn } from '@/lib/utils';

interface AmbientBackgroundProps {
  className?: string;
}

export function AmbientBackground({ className }: AmbientBackgroundProps) {
  return (
    <div
      className={cn('fixed inset-0 -z-10 overflow-hidden pointer-events-none', className)}
      aria-hidden="true"
    >
      {/* Base gradient: deepest to deep */}
      <div
        className="absolute inset-0"
        style={{
          background: 'linear-gradient(180deg, #050810 0%, #0a0e1a 40%, #0d1321 100%)',
        }}
      />

      {/* Subtle radial glow at top center */}
      <div
        className="absolute top-0 left-1/2 -translate-x-1/2 -translate-y-1/4"
        style={{
          width: '80vw',
          height: '50vh',
          background: 'radial-gradient(ellipse at center, rgba(0,212,255,0.06) 0%, rgba(59,130,246,0.03) 40%, transparent 70%)',
          filter: 'blur(40px)',
        }}
      />

      {/* Floating blurred orb — purple */}
      <div
        className="absolute animate-float"
        style={{
          top: '20%',
          left: '15%',
          width: '300px',
          height: '300px',
          background: 'radial-gradient(circle, rgba(139,92,246,0.08) 0%, transparent 70%)',
          filter: 'blur(60px)',
          borderRadius: '50%',
        }}
      />

      {/* Floating blurred orb — blue */}
      <div
        className="absolute animate-float"
        style={{
          top: '50%',
          right: '10%',
          width: '400px',
          height: '400px',
          background: 'radial-gradient(circle, rgba(59,130,246,0.06) 0%, transparent 70%)',
          filter: 'blur(80px)',
          borderRadius: '50%',
          animationDelay: '-3s',
        }}
      />

      {/* Faint dot grid pattern */}
      <div
        className="absolute inset-0"
        style={{
          backgroundImage: 'radial-gradient(circle, rgba(255,255,255,0.03) 1px, transparent 1px)',
          backgroundSize: '24px 24px',
        }}
      />
    </div>
  );
}
