"use client";

import { useParams } from "next/navigation";
import { ProjectBuild } from "@/components/project/build";

export default function BuildPage() {
  const { projectId } = useParams<{ projectId: string }>();
  return <ProjectBuild projectId={projectId} />;
}
