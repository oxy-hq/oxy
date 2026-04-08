import { useQuery } from "@tanstack/react-query";
import { RepositoryService } from "@/services/api";
import useCurrentWorkspace from "@/stores/useCurrentWorkspace";
import queryKeys from "../queryKey";

export default function useRepoBranch(name: string) {
  const { workspace: project } = useCurrentWorkspace();
  const projectId = project?.id ?? "";

  return useQuery({
    queryKey: queryKeys.repositories.branch(projectId, name),
    queryFn: () => RepositoryService.getRepoBranch(projectId, name),
    enabled: !!projectId && !!name,
    staleTime: 30_000
  });
}
