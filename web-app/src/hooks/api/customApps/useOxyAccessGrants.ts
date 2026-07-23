import { useQuery } from "@tanstack/react-query";
import { CustomAppsService } from "@/services/api/customApps";
import type { OxyAccessRow } from "@/types/apps";
import queryKeys from "../queryKey";

/**
 * Platform-wide list of workspaces that granted Oxy access, for the admin
 * Orgs / Projects browser. App-admin gated server-side (403 otherwise).
 */
export const useOxyAccessGrants = () =>
  useQuery<OxyAccessRow[]>({
    queryKey: queryKeys.oxyAccess.grants(),
    queryFn: CustomAppsService.listOxyAccess
  });
