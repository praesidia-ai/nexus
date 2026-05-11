import { useQuery } from "@tanstack/react-query";
import { api } from "@/lib/api";

export function useProjectAgents(projectId: string) {
  return useQuery({
    queryKey: ["agents", projectId],
    queryFn: () => api.listAgents(projectId),
    enabled: !!projectId,
  });
}
