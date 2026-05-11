"use client";

import { use } from "react";
import { useAgentTv } from "@/hooks/use-agent-tv";
import { AgentGrid } from "@/components/agent-tv/agent-grid";
import { SummaryBanner } from "@/components/agent-tv/summary-banner";

interface AgentTvPageProps {
  params: Promise<{ projectId: string }>;
}

export default function AgentTvPage({ params }: AgentTvPageProps) {
  const { projectId } = use(params);
  const state = useAgentTv(projectId);

  return (
    <main className="min-h-screen w-full">
      <div className="mx-auto flex min-h-screen w-full max-w-[1600px] flex-col gap-4 p-4 sm:p-6 lg:p-8">
        <header className="flex flex-col gap-1">
          <h1 className="text-2xl font-semibold tracking-tight text-slate-100 sm:text-3xl">
            Agent TV
          </h1>
          <p className="text-sm text-slate-400">
            Live view of the ten ZeroClaw agents working on this project.
          </p>
        </header>

        {state.error && (
          <div
            role="alert"
            aria-live="assertive"
            className="flex items-start gap-3 rounded-xl border border-red-500/30 bg-red-500/5 p-3 text-sm text-red-200"
          >
            <span
              aria-hidden="true"
              className="mt-0.5 inline-flex h-5 w-5 shrink-0 items-center justify-center rounded-full bg-red-500/15 text-xs text-red-300"
            >
              !
            </span>
            <div className="min-w-0">
              <p className="font-medium">Stream error</p>
              <p className="mt-0.5 break-words text-red-300/90">
                {state.error}
              </p>
            </div>
          </div>
        )}

        <AgentGrid state={state} />

        {state.complete && <SummaryBanner state={state} />}
      </div>
    </main>
  );
}
