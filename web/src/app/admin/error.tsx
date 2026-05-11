"use client";

import { useEffect } from "react";
import { AlertTriangle, RefreshCw, Home } from "lucide-react";
import Link from "next/link";

export default function ErrorBoundary({
  error,
  reset,
}: {
  error: Error & { digest?: string };
  reset: () => void;
}) {
  useEffect(() => {
    console.error("Error:", error);
  }, [error]);

  return (
    <div className="flex items-center justify-center h-full">
      <div className="max-w-md w-full mx-4">
        <div className="glass-card p-8 text-center">
          <div className="w-14 h-14 rounded-xl bg-destructive/10 border border-destructive/20 flex items-center justify-center mx-auto mb-5">
            <AlertTriangle className="w-7 h-7 text-destructive" />
          </div>
          <h1 className="text-xl font-bold mb-2">Something went wrong</h1>
          <p className="text-sm text-slate-400 mb-6">
            {error.message || "An unexpected error occurred while loading the admin panel. Please try again."}
          </p>
          <div className="flex items-center justify-center gap-3">
            <button
              onClick={reset}
              className="flex items-center gap-2 px-4 py-2.5 rounded-lg bg-gradient-to-r from-glow-cyan to-glow-blue text-white text-sm font-medium hover:brightness-110 transition-colors"
            >
              <RefreshCw className="w-4 h-4" />
              Try again
            </button>
            <Link
              href="/"
              className="flex items-center gap-2 px-4 py-2.5 rounded-lg border border-white/[0.1] text-sm text-slate-400 hover:text-slate-200 hover:bg-white/[0.05] transition-colors"
            >
              <Home className="w-4 h-4" />
              Home
            </Link>
          </div>
        </div>
      </div>
    </div>
  );
}
