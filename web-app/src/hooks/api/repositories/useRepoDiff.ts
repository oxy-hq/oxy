import { useQuery } from "@tanstack/react-query";
import { RepositoryService } from "@/services/api";
import useCurrentWorkspace from "@/stores/useCurrentWorkspace";
import queryKeys from "../queryKey";

export default function useRepoDiff(name: string, enabled = false) {
  const { workspace: project } = useCurrentWorkspace();
  const projectId = project?.id ?? "";

  return useQuery({
    queryKey: queryKeys.repositories.diff(projectId, name),
    queryFn: () => RepositoryService.getRepoDiff(projectId, name),
    enabled: !!projectId && !!name && enabled,
    staleTime: 10_000
  });
}
