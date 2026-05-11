'use client';

import { useState, useEffect, useRef, useCallback } from 'react';
import { Bell, CheckCircle, AlertTriangle, Sparkles, Bot } from 'lucide-react';
import { cn } from '@/lib/utils';

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export type NotificationType = 'success' | 'warning' | 'info' | 'agent';

export interface Notification {
  id: string;
  type: NotificationType;
  message: string;
  detail?: string;
  read: boolean;
  timestamp: number;
}

const STORAGE_KEY = 'nexus-notifications';

// ---------------------------------------------------------------------------
// Icon mapping
// ---------------------------------------------------------------------------

const TYPE_CONFIG: Record<NotificationType, { icon: typeof Bell; color: string }> = {
  success: { icon: CheckCircle, color: 'text-emerald-400' },
  warning: { icon: AlertTriangle, color: 'text-amber-400' },
  info: { icon: Sparkles, color: 'text-glow-cyan' },
  agent: { icon: Bot, color: 'text-purple-400' },
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function loadNotifications(): Notification[] {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    return raw ? JSON.parse(raw) : [];
  } catch {
    return [];
  }
}

function saveNotifications(notifications: Notification[]) {
  localStorage.setItem(STORAGE_KEY, JSON.stringify(notifications));
}

function timeAgo(ts: number): string {
  const diff = Date.now() - ts;
  const seconds = Math.floor(diff / 1000);
  if (seconds < 60) return 'just now';
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes}m ago`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours}h ago`;
  const days = Math.floor(hours / 24);
  return `${days}d ago`;
}

// ---------------------------------------------------------------------------
// NotificationCenter
// ---------------------------------------------------------------------------

interface NotificationCenterProps {
  className?: string;
}

export function NotificationCenter({ className }: NotificationCenterProps) {
  const [notifications, setNotifications] = useState<Notification[]>([]);
  const [open, setOpen] = useState(false);
  const panelRef = useRef<HTMLDivElement>(null);

  // Load from localStorage on mount
  useEffect(() => {
    setNotifications(loadNotifications());
  }, []);

  // Sync to localStorage whenever notifications change
  useEffect(() => {
    if (notifications.length > 0) {
      saveNotifications(notifications);
    }
  }, [notifications]);

  // Close on outside click
  useEffect(() => {
    function handleClick(e: MouseEvent) {
      if (panelRef.current && !panelRef.current.contains(e.target as Node)) {
        setOpen(false);
      }
    }
    if (open) {
      document.addEventListener('mousedown', handleClick);
      return () => document.removeEventListener('mousedown', handleClick);
    }
  }, [open]);

  // Listen for custom notification events (so other components can push notifications)
  useEffect(() => {
    function handlePush(e: Event) {
      const detail = (e as CustomEvent<Omit<Notification, 'id' | 'read' | 'timestamp'>>).detail;
      const notification: Notification = {
        ...detail,
        id: `n-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`,
        read: false,
        timestamp: Date.now(),
      };
      setNotifications((prev) => [notification, ...prev].slice(0, 50));
    }

    window.addEventListener('nexus-notification', handlePush);
    return () => window.removeEventListener('nexus-notification', handlePush);
  }, []);

  const unreadCount = notifications.filter((n) => !n.read).length;

  const markAsRead = useCallback((id: string) => {
    setNotifications((prev) =>
      prev.map((n) => (n.id === id ? { ...n, read: true } : n)),
    );
  }, []);

  const markAllRead = useCallback(() => {
    setNotifications((prev) => prev.map((n) => ({ ...n, read: true })));
  }, []);

  const clearAll = useCallback(() => {
    setNotifications([]);
    localStorage.removeItem(STORAGE_KEY);
  }, []);

  return (
    <div ref={panelRef} className={cn('relative', className)}>
      {/* Bell trigger */}
      <button
        type="button"
        onClick={() => setOpen(!open)}
        className={cn(
          'relative w-8 h-8 rounded-lg flex items-center justify-center transition-colors',
          'text-slate-400 hover:bg-white/[0.06] hover:text-slate-200',
          'focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-glow-cyan/40',
        )}
        aria-label={`Notifications${unreadCount > 0 ? ` (${unreadCount} unread)` : ''}`}
      >
        <Bell className="w-4 h-4" />
        {unreadCount > 0 && (
          <span className="absolute -top-0.5 -right-0.5 min-w-[16px] h-4 px-1 rounded-full bg-red-500 text-white text-[9px] font-bold flex items-center justify-center leading-none">
            {unreadCount > 9 ? '9+' : unreadCount}
          </span>
        )}
      </button>

      {/* Dropdown panel */}
      {open && (
        <div className="absolute right-0 top-full mt-2 w-80 max-h-96 rounded-xl border border-white/[0.08] bg-[#0d0d12]/95 backdrop-blur-xl shadow-2xl z-50 overflow-hidden flex flex-col">
          {/* Header */}
          <div className="flex items-center justify-between px-4 py-3 border-b border-white/[0.06]">
            <span className="text-sm font-medium text-slate-200">Notifications</span>
            <div className="flex items-center gap-2">
              {unreadCount > 0 && (
                <button
                  onClick={markAllRead}
                  className="text-[10px] text-glow-cyan hover:text-glow-cyan/80 transition-colors"
                >
                  Mark all read
                </button>
              )}
              {notifications.length > 0 && (
                <button
                  onClick={clearAll}
                  className="text-[10px] text-slate-500 hover:text-slate-400 transition-colors"
                >
                  Clear
                </button>
              )}
            </div>
          </div>

          {/* Notification list */}
          <div className="flex-1 overflow-y-auto scrollbar-thin">
            {notifications.length === 0 ? (
              <div className="flex flex-col items-center justify-center py-12 text-slate-500">
                <Bell className="w-6 h-6 mb-2 opacity-30" />
                <p className="text-xs">No notifications yet</p>
              </div>
            ) : (
              notifications.map((n) => {
                const config = TYPE_CONFIG[n.type];
                const Icon = config.icon;

                return (
                  <button
                    key={n.id}
                    type="button"
                    onClick={() => markAsRead(n.id)}
                    className={cn(
                      'w-full flex items-start gap-3 px-4 py-3 text-left transition-colors',
                      'hover:bg-white/[0.03]',
                      !n.read && 'bg-white/[0.02]',
                    )}
                  >
                    {/* Unread dot */}
                    <div className="pt-1 flex-shrink-0">
                      {!n.read ? (
                        <span className="block w-2 h-2 rounded-full bg-glow-cyan" />
                      ) : (
                        <span className="block w-2 h-2 rounded-full bg-transparent" />
                      )}
                    </div>

                    {/* Icon */}
                    <Icon className={cn('w-4 h-4 mt-0.5 flex-shrink-0', config.color)} />

                    {/* Content */}
                    <div className="flex-1 min-w-0">
                      <p className={cn(
                        'text-xs leading-relaxed',
                        n.read ? 'text-slate-400' : 'text-slate-200',
                      )}>
                        {n.message}
                      </p>
                      {n.detail && (
                        <p className="text-[10px] text-slate-500 mt-0.5 truncate">
                          {n.detail}
                        </p>
                      )}
                      <p className="text-[10px] text-slate-500/60 mt-1">
                        {timeAgo(n.timestamp)}
                      </p>
                    </div>
                  </button>
                );
              })
            )}
          </div>
        </div>
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Helper: push a notification from anywhere
// ---------------------------------------------------------------------------

export function pushNotification(
  type: NotificationType,
  message: string,
  detail?: string,
) {
  window.dispatchEvent(
    new CustomEvent('nexus-notification', {
      detail: { type, message, detail },
    }),
  );
}
