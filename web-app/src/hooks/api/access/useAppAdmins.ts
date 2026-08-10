import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { isAxiosError } from "axios";
import { toast } from "sonner";
import { AppAdminsService } from "@/services/api/access";
import type { CreateAppAdminInput, PlatformRoleId } from "@/types/access";
import queryKeys from "../queryKey";

/**
 * Display names for the platform roles. The ids are a wire contract shared with
 * `oxy_authz::PlatformRole::as_str` and persisted in the grant table; these labels are
 * only what a human reads.
 */
export const ROLE_LABELS: Record<PlatformRoleId, string> = {
  global_admin: "Global Admin",
  app_operator: "App Operator"
};

/**
 * @param enabled Pass `false` where the caller may not hold `manage_platform_grants` —
 *   the endpoint 403s for them, and a failed query behind a hidden card is noise in the
 *   console and a red line in the network tab that looks like a bug.
 */
export const useAppAdmins = (enabled = true) =>
  useQuery({
    queryKey: queryKeys.appAdmins.list(),
    queryFn: AppAdminsService.list,
    enabled
  });

export const useCreateAppAdmin = () => {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (input: CreateAppAdminInput) => AppAdminsService.create(input),
    onSuccess: (grant) => {
      qc.invalidateQueries({ queryKey: queryKeys.appAdmins.list() });
      // Report the role the SERVER returned, not the one submitted, and say "saved"
      // rather than "added" — the endpoint upserts, so this is equally the path for
      // downgrading someone. Naming the outcome from the response is what stops the
      // UI claiming a change that the server didn't make.
      const reach = grant.scope_all
        ? "all organizations"
        : `${grant.scope_org_ids.length} organization${grant.scope_org_ids.length === 1 ? "" : "s"}`;
      toast.success(`${ROLE_LABELS[grant.role] ?? grant.role} saved — ${reach}`);
    },
    onError: (err) => {
      const message = isAxiosError(err)
        ? (err.response?.data?.message ?? err.message)
        : err instanceof Error
          ? err.message
          : "Failed to add staff access";
      toast.error(message);
    }
  });
};

export const useRemoveAppAdmin = () => {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => AppAdminsService.remove(id),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: queryKeys.appAdmins.list() });
      toast.success("Staff access removed");
    },
    onError: (err) => {
      const message = isAxiosError(err)
        ? (err.response?.data?.message ?? err.message)
        : err instanceof Error
          ? err.message
          : "Failed to remove app admin";
      toast.error(message);
    }
  });
};
