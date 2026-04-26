import { useQuery } from "@tanstack/react-query";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { OnboardingService, type ReadinessResponse } from "@/services/api/onboarding";
import queryKeys from "../queryKey";

export default function useOnboardingReadiness(enabled = true) {
  const { project } = useCurrentProjectBranch();
  const projectId = project.id;
  return useQuery<ReadinessResponse, Error>({
    queryKey: queryKeys.onboarding.readiness(projectId),
    queryFn: () => OnboardingService.getReadiness(projectId),
    enabled
  });
}
