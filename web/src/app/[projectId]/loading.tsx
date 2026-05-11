import { Skeleton } from "@/components/ui/skeleton";

export default function ProjectLoading() {
  return (
    <div className="flex h-screen overflow-hidden">
      {/* Sidebar skeleton */}
      <aside className="w-[220px] flex-shrink-0 glass border-r border-white/[0.06] flex flex-col h-screen overflow-hidden">
        <div className="px-4 pt-4 pb-3 flex items-center gap-2">
          <Skeleton className="w-7 h-7 rounded-lg" />
          <Skeleton className="h-4 w-16" />
        </div>
        <div className="px-3 pb-3 border-b border-white/[0.06]">
          <div className="flex items-center gap-2 px-2 py-2">
            <Skeleton className="w-7 h-7 rounded-lg" />
            <Skeleton className="h-4 flex-1" />
          </div>
        </div>
        <div className="flex-1 py-2 space-y-1 px-2">
          {Array.from({ length: 8 }).map((_, i) => (
            <div key={i} className="flex items-center gap-2 px-2 py-1.5">
              <Skeleton className="w-3.5 h-3.5 rounded" />
              <Skeleton className="h-3 flex-1" />
            </div>
          ))}
        </div>
      </aside>
      {/* Main content skeleton */}
      <main className="flex-1 overflow-hidden p-6">
        <div className="max-w-3xl mx-auto space-y-4">
          <Skeleton className="h-8 w-48" />
          <Skeleton className="h-4 w-72" />
          <div className="space-y-3 pt-4">
            <Skeleton className="h-16 w-full rounded-xl" />
            <Skeleton className="h-16 w-3/4 rounded-xl" />
            <Skeleton className="h-16 w-5/6 rounded-xl" />
          </div>
        </div>
      </main>
    </div>
  );
}
