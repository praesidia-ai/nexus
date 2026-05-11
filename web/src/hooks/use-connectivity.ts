"use client";

import { useState, useEffect } from "react";

export function useConnectivity() {
  const [isOnline, setIsOnline] = useState(true);
  const [apiReachable, setApiReachable] = useState(true);

  useEffect(() => {
    const handleOnline = () => setIsOnline(true);
    const handleOffline = () => setIsOnline(false);
    window.addEventListener("online", handleOnline);
    window.addEventListener("offline", handleOffline);
    setIsOnline(navigator.onLine);

    const interval = setInterval(async () => {
      try {
        const res = await fetch("/api/health/live", {
          signal: AbortSignal.timeout(5000),
        });
        setApiReachable(res.ok);
      } catch {
        setApiReachable(false);
      }
    }, 15000);

    return () => {
      window.removeEventListener("online", handleOnline);
      window.removeEventListener("offline", handleOffline);
      clearInterval(interval);
    };
  }, []);

  return { isOnline, apiReachable, isFullyConnected: isOnline && apiReachable };
}
