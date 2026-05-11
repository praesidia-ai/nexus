"use client";

import { useState, useEffect, useCallback, createContext, useContext } from "react";
import { X, AlertTriangle, AlertCircle, CheckCircle2, Info } from "lucide-react";
import { cn } from "@/lib/utils";

type ToastType = "success" | "error" | "warning" | "info";

interface Toast {
  id: string;
  type: ToastType;
  title: string;
  description?: string;
}

interface ToastContextValue {
  toast: (type: ToastType, title: string, description?: string) => void;
}

const ToastContext = createContext<ToastContextValue>({
  toast: () => {},
});

export function useToast() {
  return useContext(ToastContext);
}

const ICONS: Record<ToastType, typeof Info> = {
  success: CheckCircle2,
  error: AlertTriangle,
  warning: AlertCircle,
  info: Info,
};

const STYLES: Record<ToastType, string> = {
  success: "border-emerald-500/20 bg-emerald-500/[0.08]",
  error: "border-destructive/20 bg-destructive/[0.08]",
  warning: "border-amber-500/20 bg-amber-500/[0.08]",
  info: "border-glow-cyan/20 bg-glow-cyan/[0.08]",
};

const ICON_STYLES: Record<ToastType, string> = {
  success: "text-emerald-400",
  error: "text-destructive",
  warning: "text-amber-400",
  info: "text-glow-cyan",
};

export function ToastProvider({ children }: { children: React.ReactNode }) {
  const [toasts, setToasts] = useState<Toast[]>([]);

  const addToast = useCallback(
    (type: ToastType, title: string, description?: string) => {
      const id = crypto.randomUUID();
      setToasts((prev) => [...prev, { id, type, title, description }]);
    },
    []
  );

  const removeToast = useCallback((id: string) => {
    setToasts((prev) => prev.filter((t) => t.id !== id));
  }, []);

  return (
    <ToastContext.Provider value={{ toast: addToast }}>
      {children}
      <div className="fixed bottom-4 right-4 z-[100] flex flex-col gap-2 max-w-sm">
        {toasts.map((t) => (
          <ToastItem key={t.id} toast={t} onDismiss={removeToast} />
        ))}
      </div>
    </ToastContext.Provider>
  );
}

function ToastItem({
  toast: t,
  onDismiss,
}: {
  toast: Toast;
  onDismiss: (id: string) => void;
}) {
  const [exiting, setExiting] = useState(false);

  useEffect(() => {
    const timer = setTimeout(() => {
      setExiting(true);
      setTimeout(() => onDismiss(t.id), 200);
    }, 4000);
    return () => clearTimeout(timer);
  }, [t.id, onDismiss]);

  const Icon = ICONS[t.type];

  return (
    <div
      className={cn(
        "flex items-start gap-3 px-4 py-3 rounded-xl border backdrop-blur-xl shadow-2xl transition-all duration-200",
        STYLES[t.type],
        exiting ? "opacity-0 translate-x-4" : "opacity-100 translate-x-0 animate-in slide-in-from-right-5"
      )}
    >
      <Icon className={cn("w-4 h-4 mt-0.5 flex-shrink-0", ICON_STYLES[t.type])} />
      <div className="flex-1 min-w-0">
        <p className="text-sm font-medium text-slate-200">{t.title}</p>
        {t.description && (
          <p className="text-xs text-slate-400 mt-0.5 line-clamp-2">{t.description}</p>
        )}
      </div>
      <button
        onClick={() => {
          setExiting(true);
          setTimeout(() => onDismiss(t.id), 200);
        }}
        className="text-slate-400 hover:text-slate-200 transition-colors flex-shrink-0"
      >
        <X className="w-3.5 h-3.5" />
      </button>
    </div>
  );
}
