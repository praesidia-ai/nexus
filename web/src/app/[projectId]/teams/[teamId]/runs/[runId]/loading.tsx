import { Skeleton } from "@/components/ui/skeleton";

export default function Loading() {
  return (
    <div className="h-full flex flex-col overflow-hidden">
      {/* Header bar */}
      <div className="flex-shrink-0 px-4 py-3 border-b border-white/[0.06] bg-white/[0.01] flex items-center gap-4">
        <Skeleton className="w-8 h-8 rounded-md" />
        <div className="flex-1 min-w-0 space-y-1.5">
          <div className="flex items-center gap-2">
            <Skeleton className="h-4 w-40" />
            <Skeleton className="h-5 w-16 rounded-full" />
          </div>
          <Skeleton className="h-3 w-24" />
        </div>
        {/* Timer */}
        <div className="flex items-center gap-1.5">
          <Skeleton className="w-3.5 h-3.5 rounded" />
          <Skeleton className="h-3 w-10" />
        </div>
        {/* Budget gauge */}
        <div className="flex items-center gap-2">
          <Skeleton className="w-3.5 h-3.5 rounded" />
          <div className="w-32 space-y-1">
            <div className="flex justify-between">
              <Skeleton className="h-2 w-8" />
              <Skeleton className="h-2 w-6" />
            </div>
            <Skeleton className="h-1.5 w-full rounded-full" />
          </div>
        </div>
        {/* Artifacts count */}
        <div className="flex items-center gap-1.5">
          <Skeleton className="w-3.5 h-3.5 rounded" />
          <Skeleton className="h-3 w-4" />
        </div>
      </div>

      {/* 3-panel layout */}
      <div className="flex-1 flex overflow-hidden">
        {/* Left panel: Member status */}
        <div className="w-[260px] flex-shrink-0 border-r border-white/[0.06] p-3 space-y-2">
          {Array.from({ length: 4 }).map((_, i) => (
            <div key={i} className="p-3 rounded-lg border border-white/[0.06] bg-white/[0.02]">
              <div className="flex items-center gap-2 mb-2">
                <Skeleton className="w-8 h-8 rounded-full" />
                <div className="flex-1 space-y-1">
                  <Skeleton className="h-3 w-20" />
                  <Skeleton className="h-2 w-14" />
                </div>
                <Skeleton className="h-4 w-12 rounded-full" />
              </div>
              <Skeleton className="h-1 w-full rounded-full" />
            </div>
          ))}
        </div>

        {/* Center panel: Communication feed */}
        <div className="flex-1 min-w-0 border-r border-white/[0.06] p-4 space-y-3">
          {Array.from({ length: 5 }).map((_, i) => (
            <div key={i} className="flex gap-3">
              <Skeleton className="w-7 h-7 rounded-full flex-shrink-0" />
              <div className="flex-1 space-y-1.5">
                <div className="flex items-center gap-2">
                  <Skeleton className="h-3 w-16" />
                  <Skeleton className="h-2 w-10" />
                </div>
                <Skeleton className="h-3 w-full" />
                <Skeleton className="h-3 w-3/4" />
              </div>
            </div>
          ))}
        </div>

        {/* Right panel: Task board */}
        <div className="w-[320px] flex-shrink-0 p-3 space-y-2">
          <Skeleton className="h-4 w-20 mb-3" />
          {Array.from({ length: 3 }).map((_, i) => (
            <div key={i} className="p-3 rounded-lg border border-white/[0.06] bg-white/[0.02] space-y-2">
              <div className="flex items-center justify-between">
                <Skeleton className="h-3 w-28" />
                <Skeleton className="h-4 w-14 rounded-full" />
              </div>
              <Skeleton className="h-2 w-full" />
              <Skeleton className="h-2 w-2/3" />
            </div>
          ))}
        </div>
      </div>

      {/* Bottom bar: Human input */}
      <div className="flex-shrink-0 px-4 py-3 border-t border-white/[0.06] bg-white/[0.01]">
        <div className="flex items-center gap-3">
          <Skeleton className="flex-1 h-9 rounded-lg" />
          <Skeleton className="h-9 w-20 rounded-lg" />
        </div>
      </div>
    </div>
  );
}
