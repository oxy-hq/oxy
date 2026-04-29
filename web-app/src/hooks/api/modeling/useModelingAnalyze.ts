import { useMutation } from "@tanstack/react-query";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { ModelingService } from "@/services/api/modeling";

export default function useModelingAnalyze(modelingProjectName: string) {
  const { project, branchName } = useCurrentProjectBranch();

  return useMutation({
    mutationFn: () => ModelingService.analyzeProject(project.id, modelingProjectName, branchName)
  });
}
