"use client";

import { useEffect } from "react";

export default function ObservabilityError({
  error,
  reset,
}: {
  error: Error & { digest?: string };
  reset: () => void;
}) {
  useEffect(() => {
    console.error("Observability page error:", error);
  }, [error]);

  return (
    <div className="flex flex-col items-center justify-center min-h-[400px] gap-4 p-8">
      <div className="w-12 h-12 rounded-xl bg-red-500/10 border border-red-500/20 flex items-center justify-center">
        <svg
          className="w-6 h-6 text-red-400"
          fill="none"
          viewBox="0 0 24 24"
          stroke="currentColor"
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
        Observability data unavailable
      </h2>
      <p className="text-slate-400 text-center max-w-md text-sm">
        {error.message || "Failed to load observability dashboard. The backend may be unreachable."}
      </p>
      <button
        onClick={reset}
        className="px-4 py-2 bg-white/[0.06] hover:bg-white/[0.1] text-slate-200 rounded-lg border border-white/[0.1] transition-colors text-sm"
      >
        Try again
      </button>
    </div>
  );
}
