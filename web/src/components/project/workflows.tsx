"use client";

// ProjectWorkflows: wrapper that renders the workflow canvas for a given projectId.
// We import @xyflow/react dynamically since it depends on window.

import dynamic from "next/dynamic";

const WorkflowCanvas = dynamic(() => import("./workflow-canvas"), { ssr: false });

export function ProjectWorkflows({ projectId }: { projectId: string }) {
  return <WorkflowCanvas projectId={projectId} />;
}
