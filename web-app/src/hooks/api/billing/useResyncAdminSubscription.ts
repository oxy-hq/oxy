import { useMutation, useQueryClient } from "@tanstack/react-query";
import { AdminBillingService, type ResyncSubscriptionRequest } from "@/services/api/billing";
import queryKeys from "../queryKey";

export const useResyncAdminSubscription = (orgId: string) => {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (body: ResyncSubscriptionRequest) =>
      AdminBillingService.resyncSubscription(orgId, body),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: queryKeys.adminBilling.all });
      qc.invalidateQueries({ queryKey: queryKeys.billing.org(orgId) });
    }
  });
};
