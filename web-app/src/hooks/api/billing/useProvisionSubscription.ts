import { useMutation, useQueryClient } from "@tanstack/react-query";
import { AdminBillingService, type ProvisionSubscriptionRequest } from "@/services/api/billing";
import queryKeys from "../queryKey";

export const useProvisionSubscription = (orgId: string) => {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (body: ProvisionSubscriptionRequest) =>
      AdminBillingService.provisionSubscription(orgId, body),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: queryKeys.adminBilling.all });
      qc.invalidateQueries({ queryKey: queryKeys.billing.org(orgId) });
    }
  });
};
