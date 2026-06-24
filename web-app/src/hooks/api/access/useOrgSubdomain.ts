import { useQuery } from "@tanstack/react-query";
import { OrgSubdomainService } from "@/services/api/access";
import queryKeys from "../queryKey";

/**
 * Read-only org-subdomain status for the customer's settings. Enable/disable
 * is an Oxy-staff action in the admin panel — see `useAdminOrgSubdomain`.
 */
export const useOrgSubdomain = (workspaceId: string) =>
  useQuery({
    queryKey: queryKeys.orgSubdomain.status(workspaceId),
    queryFn: () => OrgSubdomainService.get(workspaceId),
    enabled: !!workspaceId
  });
