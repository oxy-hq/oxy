import { type Query, useQuery } from "@tanstack/react-query";
import { BillingService, type OrgBillingStatus } from "@/services/api/billing";
import queryKeys from "../queryKey";

type RefetchInterval =
  | number
  | false
  | ((query: Query<OrgBillingStatus, Error>) => number | false | undefined);

interface UseOrgBillingStatusOptions {
  refetchInterval?: RefetchInterval;
}

/**
 * Member-readable billing status. Drives `OrgGuard`, `BillingBanner`, and
 * `PaywallScreen` for users of any role; admins use `useOrgBilling` for
 * the full pricing-aware summary.
 */
export const useOrgBillingStatus = (
  orgId: string,
  enabled = true,
  options: UseOrgBillingStatusOptions = {}
) => {
  return useQuery({
    queryKey: queryKeys.billing.status(orgId),
    queryFn: () => BillingService.getStatus(orgId),
    enabled: enabled && !!orgId,
    refetchInterval: options.refetchInterval ?? false
  });
};
