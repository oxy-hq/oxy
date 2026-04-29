import { useMutation } from "@tanstack/react-query";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { ModelingService } from "@/services/api/modeling";
import type { RunRequest } from "@/types/modeling";

export default function useModelingRun(modelingProjectName: string) {
  const { project, branchName } = useCurrentProjectBranch();

  return useMutation({
    mutationFn: (request: RunRequest) =>
      ModelingService.runModels(project.id, modelingProjectName, request, branchName)
  });
}
