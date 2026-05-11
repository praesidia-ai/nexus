"use client";

import { useParams, useRouter } from "next/navigation";
import { ArrowLeft, Network } from "lucide-react";
import WorkflowCanvas from "@/components/project/workflow-canvas";
import { Button } from "@/components/ui/button";

export default function ComposeWorkflowPage() {
  const params = useParams();
  const router = useRouter();
  const projectId = params.projectId as string;

  return (
    <div className="flex h-full flex-col overflow-hidden">
      <div className="flex items-center justify-between border-b border-white/[0.06] bg-nexus-deep/80 px-4 py-2.5 backdrop-blur-xl">
        <div className="flex items-center gap-3">
          <Button
            variant="ghost"
            size="sm"
            onClick={() => router.push(`/${projectId}/workflows`)}
            className="h-8 gap-1.5 text-slate-400 hover:text-white"
          >
            <ArrowLeft className="h-3.5 w-3.5" />
            Back
          </Button>
          <div className="flex items-center gap-2">
            <div className="flex h-7 w-7 items-center justify-center rounded-lg border border-violet-500/10 bg-gradient-to-br from-violet-500/20 to-cyan-500/20">
              <Network className="h-3.5 w-3.5 text-violet-400" />
            </div>
            <div>
              <h1 className="text-sm font-semibold text-slate-200">
                Compose Workflow
              </h1>
              <p className="text-[11px] text-slate-400">
                Drag agents and nodes onto the canvas, connect them, and run.
              </p>
            </div>
          </div>
        </div>
        <Button
          variant="outline"
          size="sm"
          onClick={() => router.push(`/${projectId}/agents/create`)}
          className="h-8 gap-1.5 text-xs"
        >
          Need agents? Design a team →
        </Button>
      </div>
      <div className="min-h-0 flex-1">
        <WorkflowCanvas projectId={projectId} />
      </div>
    </div>
  );
}
