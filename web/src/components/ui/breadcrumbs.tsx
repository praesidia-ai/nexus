"use client";

import { Fragment } from "react";
import Link from "next/link";
import { ChevronRight, Home } from "lucide-react";

interface BreadcrumbItem {
  label: string;
  href?: string;
}

interface BreadcrumbsProps {
  items: BreadcrumbItem[];
  className?: string;
}

export function Breadcrumbs({ items, className }: BreadcrumbsProps) {
  return (
    <nav
      className={`flex items-center gap-1.5 text-sm text-slate-500 ${className ?? ""}`}
      aria-label="Breadcrumb"
    >
      <Link
        href="/"
        className="hover:text-glow-cyan transition-colors"
        aria-label="Home"
      >
        <Home className="w-3.5 h-3.5" />
      </Link>
      {items.map((item, i) => (
        <Fragment key={i}>
          <ChevronRight className="w-3 h-3 text-slate-600 flex-shrink-0" />
          {item.href ? (
            <Link
              href={item.href}
              className="hover:text-glow-cyan transition-colors truncate max-w-[160px]"
            >
              {item.label}
            </Link>
          ) : (
            <span className="text-slate-300 truncate max-w-[200px]">
              {item.label}
            </span>
          )}
        </Fragment>
      ))}
    </nav>
  );
}
