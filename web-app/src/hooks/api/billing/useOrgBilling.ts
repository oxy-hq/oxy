import { useQuery } from "@tanstack/react-query";
import { BillingService } from "@/services/api/billing";
import queryKeys from "../queryKey";

interface UseOrgBillingOptions {
  refetchInterval?: number | false;
}

export const useOrgBilling = (
  orgId: string,
  enabled = true,
  options: UseOrgBillingOptions = {}
) => {
  return useQuery({
    queryKey: queryKeys.billing.org(orgId),
    queryFn: () => BillingService.get(orgId),
    enabled: enabled && !!orgId,
    refetchInterval: options.refetchInterval ?? false
  });
};
