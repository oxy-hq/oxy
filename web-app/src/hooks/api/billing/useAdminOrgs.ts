import { useQuery } from "@tanstack/react-query";
import { AdminBillingService, type BillingStatusId } from "@/services/api/billing";
import queryKeys from "../queryKey";

export const useAdminOrgs = (status?: BillingStatusId) => {
  return useQuery({
    queryKey: queryKeys.adminBilling.orgs(status),
    queryFn: () => AdminBillingService.listOrgs(status)
  });
};
