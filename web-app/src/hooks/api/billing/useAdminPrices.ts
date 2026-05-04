import { useQuery } from "@tanstack/react-query";
import { AdminBillingService } from "@/services/api/billing";
import queryKeys from "../queryKey";

export const useAdminPrices = () => {
  return useQuery({
    queryKey: queryKeys.adminBilling.prices(),
    queryFn: () => AdminBillingService.listPrices(),
    staleTime: 30_000
  });
};
