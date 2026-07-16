import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { isAxiosError } from "axios";
import { toast } from "sonner";
import { PartnerPublishConsentService } from "@/services/api/partnerPublishConsent";
import queryKeys from "../queryKey";

export const usePartnerPublishConsent = (orgId: string) =>
  useQuery({
    queryKey: queryKeys.partnerPublishConsent.status(orgId),
    queryFn: () => PartnerPublishConsentService.get(orgId),
    enabled: !!orgId
  });

export const useSetPartnerPublishConsent = (orgId: string) => {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (enabled: boolean) => PartnerPublishConsentService.set(orgId, enabled),
    onSuccess: (_data, enabled) => {
      qc.invalidateQueries({ queryKey: queryKeys.partnerPublishConsent.status(orgId) });
      toast.success(enabled ? "Partners may now publish apps" : "Partner publishing turned off");
    },
    onError: (err) => {
      const message = isAxiosError(err)
        ? (err.response?.data?.message ?? err.message)
        : "Failed to update partner publishing";
      toast.error(message);
    }
  });
};
