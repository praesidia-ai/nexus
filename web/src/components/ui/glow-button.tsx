'use client';

import { forwardRef } from 'react';
import { cn } from '@/lib/utils';

type GlowButtonVariant = 'primary' | 'secondary' | 'ghost' | 'danger';
type GlowButtonSize = 'sm' | 'md' | 'lg';

interface GlowButtonProps extends React.ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: GlowButtonVariant;
  size?: GlowButtonSize;
}

const variantStyles: Record<GlowButtonVariant, string> = {
  primary: cn(
    'bg-gradient-to-r from-glow-cyan to-glow-blue text-white font-medium',
    'shadow-glow-sm hover:shadow-glow-md',
    'hover:brightness-110 active:brightness-95',
  ),
  secondary: cn(
    'border border-glow-cyan/20 text-glow-cyan bg-transparent',
    'hover:bg-glow-cyan/10 hover:border-glow-cyan/30',
    'active:bg-glow-cyan/15',
  ),
  ghost: cn(
    'text-glow-white bg-transparent',
    'hover:bg-white/[0.06] hover:text-glow-cyan',
    'active:bg-white/[0.08]',
  ),
  danger: cn(
    'bg-red-500/10 text-red-400 border border-red-500/20',
    'hover:bg-red-500/20 hover:border-red-500/30',
    'active:bg-red-500/25',
  ),
};

const sizeStyles: Record<GlowButtonSize, string> = {
  sm: 'h-8 px-3 text-xs rounded-md gap-1.5',
  md: 'h-10 px-4 text-sm rounded-lg gap-2',
  lg: 'h-12 px-6 text-base rounded-lg gap-2.5',
};

export const GlowButton = forwardRef<HTMLButtonElement, GlowButtonProps>(
  ({ variant = 'primary', size = 'md', className, children, disabled, ...props }, ref) => {
    return (
      <button
        ref={ref}
        className={cn(
          'inline-flex items-center justify-center whitespace-nowrap',
          'transition-all duration-200 ease-out',
          'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-glow-cyan/40 focus-visible:ring-offset-2 focus-visible:ring-offset-nexus-deep',
          'disabled:pointer-events-none disabled:opacity-40',
          variantStyles[variant],
          sizeStyles[size],
          className,
        )}
        disabled={disabled}
        {...props}
      >
        {children}
      </button>
    );
  }
);

GlowButton.displayName = 'GlowButton';
