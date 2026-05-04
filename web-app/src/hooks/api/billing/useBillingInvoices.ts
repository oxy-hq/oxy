import { useQuery } from "@tanstack/react-query";
import { BillingService } from "@/services/api/billing";
import queryKeys from "../queryKey";

export const useBillingInvoices = (orgId: string, enabled = true) => {
  return useQuery({
    queryKey: queryKeys.billing.invoices(orgId),
    queryFn: () => BillingService.listInvoices(orgId),
    enabled: enabled && !!orgId
  });
};
