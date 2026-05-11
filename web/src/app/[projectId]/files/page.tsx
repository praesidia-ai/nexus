"use client";

import { useParams } from "next/navigation";
import { ProjectFiles } from "@/components/project/files";

export default function FilesPage() {
  const { projectId } = useParams<{ projectId: string }>();
  return <ProjectFiles projectId={projectId} />;
}
