import { useQuery } from "@tanstack/react-query";
import { RepositoryService } from "@/services/api";
import useCurrentWorkspace from "@/stores/useCurrentWorkspace";
import queryKeys from "../queryKey";

export default function useRepositories() {
  const { workspace: project } = useCurrentWorkspace();
  const projectId = project?.id ?? "";

  return useQuery({
    queryKey: queryKeys.repositories.list(projectId),
    queryFn: () => RepositoryService.listRepositories(projectId),
    enabled: !!projectId
  });
}
