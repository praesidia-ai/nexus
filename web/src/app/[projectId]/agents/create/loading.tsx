"use client";

import { Skeleton } from "@/components/ui/skeleton";

export default function Loading() {
  return (
    <div className="flex h-full flex-col">
      <div className="border-b border-white/[0.06] px-6 py-3">
        <div className="flex items-center justify-between max-w-4xl mx-auto">
          <Skeleton className="h-5 w-20" />
          <div className="flex items-center gap-3">
            {[1, 2, 3, 4].map((i) => (
              <Skeleton key={i} className="h-8 w-24 rounded-lg" />
            ))}
          </div>
          <div className="w-16" />
        </div>
      </div>
      <div className="flex-1 flex items-center justify-center p-10">
        <div className="max-w-3xl w-full space-y-8">
          <div className="flex flex-col items-center space-y-4">
            <Skeleton className="h-16 w-16 rounded-2xl" />
            <Skeleton className="h-8 w-64" />
            <Skeleton className="h-4 w-96" />
          </div>
          <Skeleton className="h-32 w-full rounded-xl" />
          <div className="grid grid-cols-3 gap-3">
            {[1, 2, 3, 4, 5, 6].map((i) => (
              <Skeleton key={i} className="h-24 rounded-xl" />
            ))}
          </div>
        </div>
      </div>
    </div>
  );
}
