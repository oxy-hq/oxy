import { useMutation, useQueryClient } from "@tanstack/react-query";
import { AdminBillingService, type ProvisionCheckoutRequest } from "@/services/api/billing";
import queryKeys from "../queryKey";

export const useProvisionCheckout = (orgId: string) => {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (body: ProvisionCheckoutRequest) =>
      AdminBillingService.provisionCheckout(orgId, body),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: queryKeys.adminBilling.all });
      qc.invalidateQueries({ queryKey: queryKeys.billing.org(orgId) });
    }
  });
};

export const useResendCheckoutEmail = (orgId: string) => {
  return useMutation({
    mutationFn: () => AdminBillingService.resendCheckout(orgId)
  });
};

export const useCancelPendingCheckout = (orgId: string) => {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: () => AdminBillingService.cancelCheckout(orgId),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: queryKeys.adminBilling.pendingCheckout(orgId) });
    }
  });
};
