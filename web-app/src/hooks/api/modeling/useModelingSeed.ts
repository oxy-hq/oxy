import { useMutation } from "@tanstack/react-query";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { ModelingService } from "@/services/api/modeling";

export default function useModelingSeed(modelingProjectName: string) {
  const { project, branchName } = useCurrentProjectBranch();

  return useMutation({
    mutationFn: () => ModelingService.seedProject(project.id, modelingProjectName, branchName)
  });
}
