import { Skeleton } from "@/components/ui/skeleton";

export default function Loading() {
  return (
    <div className="gradient-bg min-h-screen">
      <div className="max-w-4xl mx-auto px-6 py-8">
        {/* Back link */}
        <Skeleton className="h-4 w-28 mb-6" />

        {/* Page header */}
        <div className="mb-8">
          <Skeleton className="h-7 w-24 mb-2" />
          <Skeleton className="h-4 w-56" />
        </div>

        {/* Section: API Keys */}
        <div className="space-y-4 mb-10">
          <Skeleton className="h-5 w-20" />
          <div className="grid gap-4">
            {Array.from({ length: 5 }).map((_, i) => (
              <div key={i} className="rounded-xl border border-white/[0.06] bg-white/[0.02]">
                {/* Provider header */}
                <div className="p-4 pb-3">
                  <div className="flex items-center gap-3">
                    <Skeleton className="w-8 h-8 rounded-lg" />
                    <div className="flex-1 space-y-1.5">
                      <div className="flex items-center gap-2">
                        <Skeleton className="h-4 w-24" />
                        <Skeleton className="h-4 w-16 rounded-full" />
                      </div>
                      <Skeleton className="h-3 w-56" />
                    </div>
                  </div>
                </div>
                {/* API key input area */}
                <div className="px-4 pb-4">
                  <div className="flex gap-2">
                    <Skeleton className="flex-1 h-9 rounded-lg" />
                    <Skeleton className="h-9 w-16 rounded-lg" />
                  </div>
                </div>
              </div>
            ))}
          </div>
        </div>

        {/* Section: Default Model */}
        <div className="mb-10">
          <Skeleton className="h-5 w-28 mb-4" />
          <div className="rounded-xl border border-white/[0.06] bg-white/[0.02]">
            <div className="p-4 pb-3 space-y-1.5">
              <Skeleton className="h-4 w-28" />
              <Skeleton className="h-3 w-64" />
            </div>
            <div className="px-4 pb-4">
              <div className="flex flex-col sm:flex-row gap-3">
                <Skeleton className="flex-1 h-9 rounded-lg" />
                <Skeleton className="flex-1 h-9 rounded-lg" />
                <Skeleton className="h-9 w-28 rounded-lg" />
              </div>
            </div>
          </div>
        </div>

        {/* Section: Available Models */}
        <div className="mb-10">
          <Skeleton className="h-5 w-32 mb-4" />
          <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-3">
            {Array.from({ length: 9 }).map((_, i) => (
              <div key={i} className="p-4 rounded-xl border border-white/[0.06] bg-white/[0.02]">
                <div className="flex items-start justify-between gap-2 mb-2">
                  <Skeleton className="h-4 w-28" />
                  <Skeleton className="h-4 w-16 rounded-full" />
                </div>
                <div className="flex items-center gap-3">
                  <Skeleton className="h-3 w-20" />
                  <Skeleton className="h-3 w-16" />
                </div>
              </div>
            ))}
          </div>
        </div>
      </div>
    </div>
  );
}
