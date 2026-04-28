import { useQuery } from "@tanstack/react-query";
import { type SlackInstallationStatus, SlackService } from "@/services/api/slack";
import queryKeys from "../queryKey";

export const useSlackInstallation = (orgId: string) =>
  useQuery<SlackInstallationStatus>({
    queryKey: queryKeys.slack.installation(orgId),
    queryFn: () => SlackService.getInstallationStatus(orgId),
    staleTime: 30_000,
    enabled: !!orgId
  });
