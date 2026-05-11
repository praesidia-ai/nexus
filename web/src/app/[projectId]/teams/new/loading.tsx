import { Skeleton } from "@/components/ui/skeleton";

export default function Loading() {
  return (
    <div className="h-full overflow-y-auto scrollbar-thin p-8 max-w-3xl mx-auto">
      {/* Header */}
      <div className="mb-8">
        <Skeleton className="h-7 w-40 mb-2" />
        <Skeleton className="h-4 w-64" />
      </div>

      {/* Team name + description */}
      <div className="space-y-6">
        <div className="space-y-2">
          <Skeleton className="h-3 w-20" />
          <Skeleton className="h-9 w-full rounded-lg" />
        </div>
        <div className="space-y-2">
          <Skeleton className="h-3 w-24" />
          <Skeleton className="h-20 w-full rounded-lg" />
        </div>

        {/* Protocol selection */}
        <div className="space-y-2">
          <Skeleton className="h-3 w-16" />
          <div className="grid grid-cols-2 sm:grid-cols-3 gap-2">
            {Array.from({ length: 5 }).map((_, i) => (
              <div key={i} className="p-3 rounded-lg border border-white/[0.06] bg-white/[0.02]">
                <div className="flex items-center gap-2 mb-1.5">
                  <Skeleton className="w-4 h-4 rounded" />
                  <Skeleton className="h-4 w-20" />
                </div>
                <Skeleton className="h-3 w-full" />
              </div>
            ))}
          </div>
        </div>

        {/* Members section */}
        <div className="space-y-3">
          <div className="flex items-center justify-between">
            <Skeleton className="h-4 w-20" />
            <Skeleton className="h-8 w-28 rounded-lg" />
          </div>
          {Array.from({ length: 2 }).map((_, i) => (
            <div key={i} className="p-4 rounded-xl border border-white/[0.06] bg-white/[0.02] space-y-3">
              <div className="flex gap-3">
                <div className="flex-1 space-y-1.5">
                  <Skeleton className="h-3 w-12" />
                  <Skeleton className="h-9 w-full rounded-lg" />
                </div>
                <div className="flex-1 space-y-1.5">
                  <Skeleton className="h-3 w-10" />
                  <Skeleton className="h-9 w-full rounded-lg" />
                </div>
              </div>
              <div className="flex gap-3">
                <div className="flex-1 space-y-1.5">
                  <Skeleton className="h-3 w-12" />
                  <Skeleton className="h-9 w-full rounded-lg" />
                </div>
                <div className="flex-1 space-y-1.5">
                  <Skeleton className="h-3 w-10" />
                  <Skeleton className="h-9 w-full rounded-lg" />
                </div>
              </div>
            </div>
          ))}
        </div>

        {/* Budget + submit */}
        <div className="space-y-2">
          <Skeleton className="h-3 w-24" />
          <Skeleton className="h-9 w-full rounded-lg" />
        </div>
        <Skeleton className="h-10 w-full rounded-lg" />
      </div>
    </div>
  );
}
