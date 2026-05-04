import { useQuery } from "@tanstack/react-query";
import { BillingService } from "@/services/api/billing";
import queryKeys from "../queryKey";

/**
 * Verifies a Stripe Checkout Session belongs to the org and has been paid.
 * Returned by the customer-facing `/billing/checkout-success` page after the
 * Stripe redirect, before polling billing status. Independent of the webhook.
 */
export const useCheckoutSession = (orgId: string, sessionId: string, enabled = true) => {
  return useQuery({
    queryKey: queryKeys.billing.checkoutSession(orgId, sessionId),
    queryFn: () => BillingService.getCheckoutSession(orgId, sessionId),
    enabled: enabled && !!orgId && !!sessionId,
    staleTime: 30_000
  });
};
