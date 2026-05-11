'use client';

import { useState } from 'react';
import { Monitor, Smartphone, Tablet, Maximize2, Minimize2, RefreshCw, ExternalLink } from 'lucide-react';
import { buildAppPreviewUrl, cn } from '@/lib/utils';
import { Button } from '@/components/ui/button';

interface AppPreviewProps {
  port: number;
  status: string;
  /**
   * Explicit URL from the backend (emitted on the Complete SSE event).
   * Takes precedence over `http://localhost:${port}` so the preview works
   * even when the backend is on a remote host / different origin.
   */
  url?: string | null;
  className?: string;
}

const DEVICES = [
  { id: 'desktop', icon: Monitor, label: 'Desktop', width: '100%', maxWidth: '100%' },
  { id: 'tablet', icon: Tablet, label: 'Tablet', width: '768px', maxWidth: '768px' },
  { id: 'mobile', icon: Smartphone, label: 'Mobile', width: '375px', maxWidth: '375px' },
] as const;

export function AppPreview({ port, status, url: providedUrl, className }: AppPreviewProps) {
  const [device, setDevice] = useState<string>('desktop');
  const [expanded, setExpanded] = useState(false);
  const [key, setKey] = useState(0); // for iframe refresh

  const url = buildAppPreviewUrl(port, providedUrl) ?? `http://localhost:${port}`;
  const isRunning = status === 'running';
  const currentDevice = DEVICES.find(d => d.id === device) || DEVICES[0];

  if (!isRunning) {
    return (
      <div className={cn("rounded-xl border border-white/10 bg-white/[0.02] flex items-center justify-center p-12", className)}>
        <div className="text-center">
          <Monitor className="w-12 h-12 text-slate-400/30 mx-auto mb-3" />
          <p className="text-slate-400 text-sm">Start your app to see the live preview</p>
        </div>
      </div>
    );
  }

  return (
    <div className={cn(
      "rounded-xl border border-white/10 bg-white/[0.02] overflow-hidden",
      expanded && "fixed inset-4 z-50 border-primary/30 neon-glow",
      className
    )}>
      {/* Toolbar */}
      <div className="flex items-center justify-between px-4 py-2 border-b border-white/10 bg-white/[0.03]">
        <div className="flex items-center gap-1">
          {DEVICES.map(d => (
            <Button
              key={d.id}
              variant={device === d.id ? "secondary" : "ghost"}
              size="icon"
              className="h-7 w-7"
              onClick={() => setDevice(d.id)}
              title={d.label}
            >
              <d.icon className="w-3.5 h-3.5" />
            </Button>
          ))}
        </div>

        <div className="flex items-center gap-2">
          <span className="text-xs text-slate-400 font-mono">{url}</span>
          <div className="w-1.5 h-1.5 rounded-full bg-emerald-400 animate-pulse" />
        </div>

        <div className="flex items-center gap-1">
          <Button variant="ghost" size="icon" className="h-7 w-7" onClick={() => setKey(k => k + 1)} title="Refresh" aria-label="Refresh preview">
            <RefreshCw className="w-3.5 h-3.5" />
          </Button>
          <Button variant="ghost" size="icon" className="h-7 w-7" onClick={() => setExpanded(!expanded)} title={expanded ? "Minimize" : "Maximize"} aria-label={expanded ? "Minimize preview" : "Maximize preview"}>
            {expanded ? <Minimize2 className="w-3.5 h-3.5" /> : <Maximize2 className="w-3.5 h-3.5" />}
          </Button>
          <Button variant="ghost" size="icon" className="h-7 w-7" asChild title="Open in new tab" aria-label="Open preview in a new tab">
            <a href={url} target="_blank" rel="noopener noreferrer">
              <ExternalLink className="w-3.5 h-3.5" />
            </a>
          </Button>
        </div>
      </div>

      {/* iframe container */}
      <div className="flex justify-center bg-[#0d0d12] p-2" style={{ minHeight: expanded ? 'calc(100vh - 8rem)' : '500px' }}>
        <div style={{ width: currentDevice.width, maxWidth: currentDevice.maxWidth, transition: 'width 0.3s ease' }} className="h-full">
          {/*
            Sandbox note: previewed apps run on localhost:PORT (a different
            origin than the Nexus UI), so we do NOT grant allow-same-origin.
            Granting it would give the preview access to document.cookie /
            localStorage on the parent origin, which can include JWTs.
            referrerpolicy keeps the preview from learning what page is
            embedding it.
          */}
          <iframe
            key={key}
            src={url}
            className="w-full h-full rounded-lg border border-white/5"
            style={{ minHeight: expanded ? 'calc(100vh - 10rem)' : '480px', background: '#fff' }}
            title="App Preview"
            sandbox="allow-scripts allow-forms allow-popups"
            referrerPolicy="no-referrer"
          />
        </div>
      </div>
    </div>
  );
}
