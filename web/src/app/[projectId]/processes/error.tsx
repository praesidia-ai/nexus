"use client";
import { AlertTriangle } from "lucide-react";
import { Button } from "@/components/ui/button";
export default function Error({ error, reset }: { error: Error; reset: () => void }) {
  return (
    <div className="flex flex-col items-center justify-center h-[60vh] gap-4">
      <AlertTriangle className="w-12 h-12 text-amber-400" />
      <h2 className="text-lg font-medium text-slate-200">Something went wrong</h2>
      <p className="text-sm text-slate-400 max-w-md text-center">{error.message}</p>
      <Button variant="outline" onClick={reset}>Try again</Button>
    </div>
  );
}
