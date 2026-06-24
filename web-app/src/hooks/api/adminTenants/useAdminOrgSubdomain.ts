import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { isAxiosError } from "axios";
import { toast } from "sonner";
import { AdminOrgsService, type SetAdminOrgSubdomainBody } from "@/services/api/adminTenants";
import queryKeys from "../queryKey";

/** Oxy-staff view of an org's bare subdomain (enabled state + default project). */
export const useAdminOrgSubdomain = (orgId: string | undefined) =>
  useQuery({
    queryKey: queryKeys.adminOrgs.subdomain(orgId ?? ""),
    queryFn: () => AdminOrgsService.getSubdomain(orgId as string),
    enabled: !!orgId
  });

/** Enable/disable an org's subdomain + set its default project (Oxy staff). */
export const useSetAdminOrgSubdomain = () => {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ orgId, body }: { orgId: string; body: SetAdminOrgSubdomainBody }) =>
      AdminOrgsService.setSubdomain(orgId, body),
    onSuccess: (_data, vars) => {
      qc.invalidateQueries({ queryKey: queryKeys.adminOrgs.subdomain(vars.orgId) });
      toast.success("Org subdomain updated");
    },
    onError: (err) => {
      const message = isAxiosError(err)
        ? err.response?.status === 409
          ? "That org slug is a reserved label and can't be used as a subdomain."
          : (err.response?.data?.message ?? err.message)
        : err instanceof Error
          ? err.message
          : "Failed to update org subdomain";
      toast.error(message);
    }
  });
};
