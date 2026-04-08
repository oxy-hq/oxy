import { useQuery } from "@tanstack/react-query";
import { RepositoryService } from "@/services/api";
import useCurrentWorkspace from "@/stores/useCurrentWorkspace";
import queryKeys from "../queryKey";

export default function useRepoBranches(name: string, enabled = true) {
  const { workspace: project } = useCurrentWorkspace();
  const projectId = project?.id ?? "";

  return useQuery({
    queryKey: queryKeys.repositories.branches(projectId, name),
    queryFn: () => RepositoryService.listRepoBranches(projectId, name),
    enabled: !!projectId && !!name && enabled,
    staleTime: 30_000
  });
}
