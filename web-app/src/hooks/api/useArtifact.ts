import { useQuery } from "@tanstack/react-query";
import { ArtifactService } from "@/services/api";
import useCurrentProjectBranch from "../useCurrentProjectBranch";
import queryKeys from "./queryKey";

export const useArtifact = (id: string) => {
  const { project, branchName } = useCurrentProjectBranch();
  const projectId = project.id;

  return useQuery({
    queryKey: queryKeys.artifact.get(projectId, branchName, id),
    queryFn: () => ArtifactService.getArtifact(projectId, branchName, id)
  });
};
