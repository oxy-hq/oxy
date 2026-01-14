import { useQuery } from "@tanstack/react-query";
import { TracesService, ClusterMapResponse } from "@/services/api/traces";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";

export default function useClusterMap(
  limit: number = 500,
  days: number = 30,
  enabled: boolean = true,
  source?: string,
) {
  const { project } = useCurrentProjectBranch();
  const projectId = project?.id;

  return useQuery<ClusterMapResponse>({
    queryKey: ["clusterMap", projectId, limit, days, source],
    queryFn: () => TracesService.getClusterMap(projectId!, limit, days, source),
    enabled: enabled && !!projectId,
  });
}
