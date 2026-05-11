import { Skeleton } from "@/components/ui/skeleton";

/**
 * Shared route-level loading skeleton. Used by subroutes that don't ship
 * their own `loading.tsx`. Keeps the workspace chrome visible while the
 * route's data is in flight, avoiding the blank-flash that otherwise
 * appears between clicks.
 */
export function RouteLoading() {
  return (
    <div className="p-6 space-y-6">
      <div className="flex items-center gap-3">
        <Skeleton className="h-9 w-9 rounded-lg" />
        <div className="space-y-2">
          <Skeleton className="h-5 w-48" />
          <Skeleton className="h-3 w-64" />
        </div>
      </div>
      <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
        {Array.from({ length: 3 }).map((_, i) => (
          <Skeleton key={i} className="h-24 rounded-xl" />
        ))}
      </div>
      <Skeleton className="h-64 rounded-xl" />
    </div>
  );
}
