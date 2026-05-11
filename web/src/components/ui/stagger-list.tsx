'use client';

import { cn } from '@/lib/utils';

interface StaggerListProps {
  children: React.ReactNode;
  className?: string;
  /** Delay between each item in ms (default: 60) */
  staggerMs?: number;
}

export function StaggerList({ children, className, staggerMs = 60 }: StaggerListProps) {
  return (
    <div className={cn('flex flex-col', className)}>
      {Array.isArray(children)
        ? children.map((child, i) => (
            <div
              key={i}
              className="opacity-0 animate-fade-in-up"
              style={{ animationDelay: `${i * staggerMs}ms` }}
            >
              {child}
            </div>
          ))
        : children}
    </div>
  );
}

interface StaggerItemProps {
  children: React.ReactNode;
  index: number;
  staggerMs?: number;
  className?: string;
}

export function StaggerItem({ children, index, staggerMs = 60, className }: StaggerItemProps) {
  return (
    <div
      className={cn('opacity-0 animate-fade-in-up', className)}
      style={{ animationDelay: `${index * staggerMs}ms` }}
    >
      {children}
    </div>
  );
}
