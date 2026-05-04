import { useMutation } from "@tanstack/react-query";
import { toast } from "sonner";
import { BillingService } from "@/services/api/billing";

export const useCreatePortalSession = (orgId: string) => {
  return useMutation({
    mutationFn: () => BillingService.createPortalSession(orgId),
    onSuccess: (data) => {
      window.location.href = data.url;
    },
    onError: (error) => {
      console.error("Failed to open billing portal:", error);
      toast.error("Failed to open billing portal", {
        description: error instanceof Error ? error.message : undefined
      });
    }
  });
};
