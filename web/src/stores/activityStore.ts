/**
 * Simple global activity feed store using an external-sync pattern.
 * Components call `subscribe()` to react to changes.
 */

export interface ActivityItem {
  id: string;
  type: string;
  message: string;
  timestamp: string;
  projectId?: string;
  metadata?: Record<string, unknown>;
}

type Listener = () => void;

let items: ActivityItem[] = [];
let idCounter = 0;
const listeners = new Set<Listener>();

function notify() {
  listeners.forEach((fn) => fn());
}

export const activityStore = {
  /** Add an activity item to the feed */
  add(item: Omit<ActivityItem, "id" | "timestamp">): ActivityItem {
    const entry: ActivityItem = {
      ...item,
      id: String(++idCounter),
      timestamp: new Date().toISOString(),
    };
    items = [entry, ...items].slice(0, 200); // keep latest 200
    notify();
    return entry;
  },

  /** Get all activity items (newest first) */
  list(): ActivityItem[] {
    return items;
  },

  /** Get items filtered by project */
  listByProject(projectId: string): ActivityItem[] {
    return items.filter((i) => i.projectId === projectId);
  },

  /** Clear all items */
  clear() {
    items = [];
    notify();
  },

  /** Subscribe to changes. Returns unsubscribe function. */
  subscribe(listener: Listener): () => void {
    listeners.add(listener);
    return () => {
      listeners.delete(listener);
    };
  },

  /** Get snapshot for useSyncExternalStore */
  getSnapshot(): ActivityItem[] {
    return items;
  },
};
