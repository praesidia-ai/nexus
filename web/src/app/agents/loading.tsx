import { Skeleton } from "@/components/ui/skeleton";

export default function Loading() {
  return (
    <div className="flex h-full overflow-hidden">
      <div className="flex-1 overflow-y-auto scrollbar-thin p-8 max-w-5xl mx-auto">
        {/* Header */}
        <div className="mb-8">
          <Skeleton className="h-7 w-28 mb-2" />
          <Skeleton className="h-4 w-52" />
        </div>

        {/* Section: Running Now */}
        <div className="space-y-8">
          <section>
            <div className="flex items-center gap-2 mb-4">
              <Skeleton className="w-4 h-4 rounded" />
              <Skeleton className="h-3 w-24" />
              <Skeleton className="h-4 w-6 rounded-full" />
            </div>
            <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-3">
              {Array.from({ length: 3 }).map((_, i) => (
                <div key={i} className="p-4 rounded-xl border border-white/[0.06] bg-white/[0.02]">
                  <div className="flex items-center gap-3">
                    <Skeleton className="w-10 h-10 rounded-lg" />
                    <div className="flex-1 space-y-2">
                      <Skeleton className="h-4 w-24" />
                      <Skeleton className="h-3 w-16" />
                      <Skeleton className="h-2 w-20" />
                    </div>
                    <Skeleton className="w-4 h-4" />
                  </div>
                </div>
              ))}
            </div>
          </section>

          {/* Section: Reactive */}
          <section>
            <div className="flex items-center gap-2 mb-4">
              <Skeleton className="w-4 h-4 rounded" />
              <Skeleton className="h-3 w-20" />
              <Skeleton className="h-4 w-6 rounded-full" />
            </div>
            <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-3">
              {Array.from({ length: 3 }).map((_, i) => (
                <div key={i} className="p-4 rounded-xl border border-white/[0.06] bg-white/[0.02]">
                  <div className="flex items-center gap-3">
                    <Skeleton className="w-10 h-10 rounded-lg" />
                    <div className="flex-1 space-y-2">
                      <Skeleton className="h-4 w-24" />
                      <Skeleton className="h-3 w-16" />
                      <Skeleton className="h-2 w-20" />
                    </div>
                    <Skeleton className="w-4 h-4" />
                  </div>
                </div>
              ))}
            </div>
          </section>
        </div>
      </div>
    </div>
  );
}
