import { useMutation } from "@tanstack/react-query";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import type { LookerQueryRequest } from "@/services/api/integrations";
import { IntegrationService } from "@/services/api/integrations";

export default function useExecuteLookerQuery() {
  const { project, branchName } = useCurrentProjectBranch();
  const projectId = project.id;

  return useMutation({
    mutationFn: (request: LookerQueryRequest) =>
      IntegrationService.executeLookerQuery(projectId, branchName, request)
  });
}
