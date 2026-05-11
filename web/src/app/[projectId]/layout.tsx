import { NexusSidebar } from "@/components/nexus-sidebar";
import { api } from "@/lib/api";

interface Props {
  children: React.ReactNode;
  params: Promise<{ projectId: string }>;
}

export default async function ProjectLayout({ children, params }: Props) {
  const { projectId } = await params;

  let projectName = "Project";
  try {
    const project = await api.getProject(projectId);
    projectName = project.name;
  } catch {
    // fallback
  }

  return (
    <div className="flex h-full w-full overflow-hidden">
      <NexusSidebar projectId={projectId} projectName={projectName} />
      <div className="flex-1 overflow-hidden page-enter">{children}</div>
    </div>
  );
}
