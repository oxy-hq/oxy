import { useQuery } from "@tanstack/react-query";
import { CustomAppsService } from "@/services/api/customApps";
import queryKeys from "../queryKey";

/**
 * The curated scaffold templates (`GET /customer-apps/templates`). A static
 * registry on the server, so cached for the session; `enabled` lets the create
 * dialog fetch only while it is open.
 */
export const useAppTemplates = (enabled = true) =>
  useQuery({
    queryKey: queryKeys.customApps.templates(),
    queryFn: () => CustomAppsService.listTemplates(),
    enabled,
    staleTime: Number.POSITIVE_INFINITY
  });
