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

/** Read a Blob as a `data:` URL — cache-friendly (a string, unlike an object
 *  URL that needs revoking) and small enough for the ≤1 MB logo cap. */
function blobToDataUrl(blob: Blob): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(reader.result as string);
    reader.onerror = () => reject(reader.error);
    reader.readAsDataURL(blob);
  });
}

/** The org's uploaded logo as a `data:` URL, or `null` if none. */
export const useAdminOrgLogo = (orgId: string | undefined) =>
  useQuery({
    queryKey: queryKeys.adminOrgs.logo(orgId ?? ""),
    queryFn: async () => {
      const blob = await AdminOrgsService.getLogo(orgId as string);
      return blob ? await blobToDataUrl(blob) : null;
    },
    enabled: !!orgId,
    staleTime: 5 * 60_000
  });

export const useUploadAdminOrgLogo = () => {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ orgId, file }: { orgId: string; file: File }) =>
      AdminOrgsService.uploadLogo(orgId, file),
    onSuccess: (_data, vars) => {
      qc.invalidateQueries({ queryKey: queryKeys.adminOrgs.logo(vars.orgId) });
      qc.invalidateQueries({ queryKey: queryKeys.adminOrgs.detail(vars.orgId) });
      qc.invalidateQueries({ queryKey: queryKeys.adminOrgs.all });
      toast.success("Logo updated");
    },
    onError: (err) => toast.error(errMessage(err, "Failed to update logo"))
  });
};

export const useDeleteAdminOrgLogo = () => {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (orgId: string) => AdminOrgsService.deleteLogo(orgId),
    onSuccess: (_data, orgId) => {
      qc.invalidateQueries({ queryKey: queryKeys.adminOrgs.logo(orgId) });
      qc.invalidateQueries({ queryKey: queryKeys.adminOrgs.detail(orgId) });
      qc.invalidateQueries({ queryKey: queryKeys.adminOrgs.all });
      toast.success("Logo removed");
    },
    onError: (err) => toast.error(errMessage(err, "Failed to remove logo"))
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
