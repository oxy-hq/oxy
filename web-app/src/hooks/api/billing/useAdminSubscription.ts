import { useQuery } from "@tanstack/react-query";
import { AdminBillingService } from "@/services/api/billing";
import queryKeys from "../queryKey";

export const useAdminSubscription = (orgId: string | null) => {
  return useQuery({
    queryKey: queryKeys.adminBilling.subscription(orgId ?? ""),
    queryFn: () => AdminBillingService.getSubscription(orgId as string),
    enabled: !!orgId
  });
};
