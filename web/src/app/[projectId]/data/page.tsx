"use client";

import { useParams } from "next/navigation";
import { ProjectData } from "@/components/project/data";

export default function DataPage() {
  const { projectId } = useParams<{ projectId: string }>();
  return <ProjectData projectId={projectId} />;
}
