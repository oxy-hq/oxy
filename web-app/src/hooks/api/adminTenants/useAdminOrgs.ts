import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { isAxiosError } from "axios";
import { toast } from "sonner";
import {
  AdminOrgsService,
  type ListOrgsMetaQuery,
  type RenameOrgBody
} from "@/services/api/adminTenants";
import queryKeys from "../queryKey";

function errMessage(err: unknown, fallback: string): string {
  if (isAxiosError(err)) return err.response?.data?.message ?? err.message;
  if (err instanceof Error) return err.message;
  return fallback;
}

export const useAdminOrgsList = (
  query: ListOrgsMetaQuery = {},
  options: { enabled?: boolean } = {}
) =>
  useQuery({
    queryKey: queryKeys.adminOrgs.list(query.search),
    queryFn: () => AdminOrgsService.list(query),
    enabled: options.enabled ?? true
  });

export const useAdminOrgDetail = (orgId: string | undefined) =>
  useQuery({
    queryKey: queryKeys.adminOrgs.detail(orgId ?? ""),
    queryFn: () => AdminOrgsService.detail(orgId as string),
    enabled: !!orgId
  });

export const useRenameAdminOrg = () => {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ orgId, body }: { orgId: string; body: RenameOrgBody }) =>
      AdminOrgsService.rename(orgId, body),
    onSuccess: (_data, vars) => {
      qc.invalidateQueries({ queryKey: queryKeys.adminOrgs.all });
      qc.invalidateQueries({ queryKey: queryKeys.adminOrgs.detail(vars.orgId) });
      toast.success("Organization updated");
    },
    onError: (err) => toast.error(errMessage(err, "Failed to update organization"))
  });
};

export const useDeleteAdminOrg = () => {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (orgId: string) => AdminOrgsService.remove(orgId),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: queryKeys.adminOrgs.all });
      toast.success("Organization deleted");
    },
    onError: (err) => toast.error(errMessage(err, "Failed to delete organization"))
  });
};

export const useTransferOrgOwnership = () => {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ orgId, newOwnerUserId }: { orgId: string; newOwnerUserId: string }) =>
      AdminOrgsService.transferOwnership(orgId, newOwnerUserId),
    onSuccess: (_data, vars) => {
      qc.invalidateQueries({ queryKey: queryKeys.adminOrgs.detail(vars.orgId) });
      qc.invalidateQueries({ queryKey: queryKeys.adminOrgs.all });
      toast.success("Ownership transferred");
    },
    onError: (err) => toast.error(errMessage(err, "Failed to transfer ownership"))
  });
};
