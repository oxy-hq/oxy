import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { CustomAppsService } from "@/services/api/customApps";
import { errMessage } from "../errMessage";
import queryKeys from "../queryKey";

/**
 * The customer-owned domains an app is served on, with the CNAME target they must
 * point at (`GET /customer-apps/{id}/domains`).
 */
export const useAppDomains = (appId: string | undefined) =>
  useQuery({
    queryKey: queryKeys.customApps.domains(appId ?? ""),
    queryFn: () => CustomAppsService.listDomains(appId as string),
    enabled: !!appId,
    staleTime: 15_000
  });

/** Every write refetches the list: add and remove change it, verify changes a row. */
const useInvalidateDomains = (appId: string) => {
  const qc = useQueryClient();
  return () => {
    void qc.invalidateQueries({ queryKey: queryKeys.customApps.domains(appId) });
  };
};

export const useAddAppDomain = (appId: string) => {
  const invalidate = useInvalidateDomains(appId);
  return useMutation({
    mutationFn: (domain: string) => CustomAppsService.addDomain(appId, domain),
    onSuccess: (d) => {
      invalidate();
      toast.success(`${d.domain} added`);
    },
    onError: (err) => toast.error(errMessage(err, "Couldn't add the domain"))
  });
};

export const useVerifyAppDomain = (appId: string) => {
  const invalidate = useInvalidateDomains(appId);
  return useMutation({
    mutationFn: (domainId: string) => CustomAppsService.verifyDomain(appId, domainId),
    onSuccess: (d) => {
      invalidate();
      // A 200 can still be pending: the row's `last_check_error` says why, so the
      // toast only celebrates the verified case.
      if (d.status === "verified") toast.success(`${d.domain} verified`);
    },
    onError: (err) => {
      // A 409 (another app verified it first) leaves the row as it was; refetch anyway.
      invalidate();
      toast.error(errMessage(err, "Couldn't verify the domain"));
    }
  });
};

export const useRemoveAppDomain = (appId: string) => {
  const invalidate = useInvalidateDomains(appId);
  return useMutation({
    mutationFn: (domainId: string) => CustomAppsService.removeDomain(appId, domainId),
    onSuccess: () => {
      invalidate();
      toast.success("Domain removed");
    },
    onError: (err) => toast.error(errMessage(err, "Couldn't remove the domain"))
  });
};
