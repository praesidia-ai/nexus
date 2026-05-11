import { Skeleton } from "@/components/ui/skeleton";

export default function Loading() {
  return (
    <div className="h-screen overflow-y-auto scrollbar-thin p-8 max-w-7xl mx-auto">
      {/* Header */}
      <div className="mb-6">
        <Skeleton className="h-7 w-24 mb-2" />
        <Skeleton className="h-4 w-72" />
      </div>

      {/* Tab bar skeleton */}
      <div className="flex items-center gap-1 p-1 rounded-lg bg-white/[0.03] border border-white/[0.06] w-fit mb-6">
        {Array.from({ length: 6 }).map((_, i) => (
          <Skeleton key={i} className="h-8 w-24 rounded-md" />
        ))}
      </div>

      {/* Stats cards (3 columns like HealthPanel) */}
      <div className="space-y-4">
        <div className="grid grid-cols-3 gap-4">
          {Array.from({ length: 3 }).map((_, i) => (
            <div key={i} className="p-4 rounded-xl border border-white/[0.06] bg-white/[0.02] text-center">
              <Skeleton className="h-3 w-20 mx-auto mb-3" />
              <Skeleton className="h-9 w-12 mx-auto" />
            </div>
          ))}
        </div>

        {/* System status card */}
        <div className="rounded-xl border border-white/[0.06] bg-white/[0.02]">
          <div className="p-4 border-b border-white/[0.06]">
            <Skeleton className="h-4 w-28" />
          </div>
          <div className="p-4 space-y-3">
            {Array.from({ length: 3 }).map((_, i) => (
              <div key={i} className="flex items-center justify-between">
                <Skeleton className="h-4 w-28" />
                <div className="flex items-center gap-2">
                  <Skeleton className="h-2 w-2 rounded-full" />
                  <Skeleton className="h-5 w-14 rounded-full" />
                </div>
              </div>
            ))}
          </div>
        </div>
      </div>
    </div>
  );
}
