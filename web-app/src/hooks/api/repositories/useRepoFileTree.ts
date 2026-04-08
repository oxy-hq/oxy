import { useQuery } from "@tanstack/react-query";
import { RepositoryService } from "@/services/api";
import useCurrentWorkspace from "@/stores/useCurrentWorkspace";

export default function useRepoFileTree(
  name: string,
  enabled = false,
  syncStatus?: "ready" | "cloning"
) {
  const { workspace: project } = useCurrentWorkspace();
  const projectId = project?.id ?? "";
  const isCloning = syncStatus === "cloning";

  return useQuery({
    queryKey: ["repositories", "files", projectId, name],
    queryFn: () => RepositoryService.getRepoFileTree(projectId, name),
    enabled: enabled && !!projectId,
    // While cloning: poll every 3 s with no stale cache so we pick up files the moment they land.
    staleTime: isCloning ? 0 : 30_000,
    refetchInterval: isCloning ? 3_000 : false
  });
}
