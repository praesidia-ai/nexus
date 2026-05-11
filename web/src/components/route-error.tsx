"use client";

import { useEffect } from "react";

/**
 * Shared route-level error boundary component. Routes under [projectId]/
 * without a custom error.tsx fall back to this one instead of the global
 * boundary — keeping the sidebar visible and giving the user a scoped
 * "try again" button.
 */
export interface RouteErrorProps {
  error: Error & { digest?: string };
  reset: () => void;
  routeLabel: string;
}

export function RouteError({ error, reset, routeLabel }: RouteErrorProps) {
  useEffect(() => {
    console.error(`[${routeLabel}] route error:`, error);
  }, [error, routeLabel]);

  return (
    <div className="flex flex-col items-center justify-center min-h-[400px] gap-4 p-8">
      <div className="w-12 h-12 rounded-xl bg-red-500/10 border border-red-500/20 flex items-center justify-center">
        <svg
          className="w-6 h-6 text-red-400"
          fill="none"
          viewBox="0 0 24 24"
          stroke="currentColor"
          aria-hidden="true"
        >
          <path
            strokeLinecap="round"
            strokeLinejoin="round"
            strokeWidth={2}
            d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-2.5L13.732 4c-.77-.833-1.964-.833-2.732 0L4.082 16.5c-.77.833.192 2.5 1.732 2.5z"
          />
        </svg>
      </div>
      <h2 className="text-xl font-semibold text-slate-200">
        {routeLabel} unavailable
      </h2>
      <p className="text-slate-300 text-center max-w-md text-sm">
        {error.message ||
          "Something went wrong loading this page. The backend may be unreachable."}
      </p>
      <button
        type="button"
        onClick={reset}
        className="px-4 py-2 bg-white/[0.06] hover:bg-white/[0.1] text-slate-200 rounded-lg border border-white/[0.1] transition-colors text-sm"
      >
        Try again
      </button>
    </div>
  );
}
