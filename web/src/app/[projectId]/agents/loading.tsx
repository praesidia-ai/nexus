"use client";

import { Skeleton } from "@/components/ui/skeleton";

export default function Loading() {
  return (
    <div className="flex h-full flex-col">
      <div className="border-b border-white/[0.06] px-8 py-5">
        <Skeleton className="mb-2 h-7 w-48" />
        <Skeleton className="h-4 w-80" />
      </div>
      <div className="flex-1 overflow-auto p-6">
        <div className="mx-auto flex max-w-3xl flex-col gap-4">
          {[1, 2, 3].map((i) => (
            <div
              key={i}
              className="flex flex-col gap-3 rounded-lg border border-white/[0.06] p-4 md:flex-row md:items-center"
            >
              <Skeleton className="h-12 w-12 shrink-0 rounded-lg" />
              <div className="min-w-0 flex-1 space-y-2">
                <div className="flex flex-wrap items-center gap-2">
                  <Skeleton className="h-5 w-40" />
                  <Skeleton className="h-5 w-16 rounded-full" />
                </div>
                <Skeleton className="h-3 w-full max-w-lg" />
                <Skeleton className="h-3 w-2/3 max-w-md" />
              </div>
              <div className="flex shrink-0 gap-2 md:flex-col">
                <Skeleton className="h-8 w-20" />
                <Skeleton className="h-8 w-20" />
              </div>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}
