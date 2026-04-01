import { useQuery } from "@tanstack/react-query";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import type { LookerIntegrationInfo } from "@/services/api";
import { IntegrationService } from "@/services/api";
import queryKeys from "../queryKey";

export default function useLookerIntegrations() {
  const { project, branchName } = useCurrentProjectBranch();
  const projectId = project.id;
  return useQuery<LookerIntegrationInfo[], Error>({
    queryKey: queryKeys.integration.looker(projectId, branchName),
    queryFn: () => IntegrationService.listLookerIntegrations(projectId, branchName)
  });
}
