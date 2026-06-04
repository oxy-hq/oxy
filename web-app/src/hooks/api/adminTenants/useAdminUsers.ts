import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { isAxiosError } from "axios";
import { toast } from "sonner";
import {
  AdminUsersService,
  type ListUsersQuery,
  type OrgRoleId,
  type UserStatusId
} from "@/services/api/adminTenants";
import queryKeys from "../queryKey";

function errMessage(err: unknown, fallback: string): string {
  if (isAxiosError(err)) return err.response?.data?.message ?? err.message;
  if (err instanceof Error) return err.message;
  return fallback;
}

export const useAdminUsersList = (
  query: ListUsersQuery = {},
  options: { enabled?: boolean } = {}
) =>
  useQuery({
    queryKey: queryKeys.adminUsers.list(query.search, query.status),
    queryFn: () => AdminUsersService.list(query),
    enabled: options.enabled ?? true
  });

export const useAdminUserDetail = (userId: string | undefined) =>
  useQuery({
    queryKey: queryKeys.adminUsers.detail(userId ?? ""),
    queryFn: () => AdminUsersService.detail(userId as string),
    enabled: !!userId
  });

export const useSetUserStatus = () => {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ userId, status }: { userId: string; status: UserStatusId }) =>
      AdminUsersService.setStatus(userId, status),
    onSuccess: (_data, vars) => {
      qc.invalidateQueries({ queryKey: queryKeys.adminUsers.all });
      qc.invalidateQueries({ queryKey: queryKeys.adminUsers.detail(vars.userId) });
      toast.success("User status updated");
    },
    onError: (err) => toast.error(errMessage(err, "Failed to update user status"))
  });
};

export const useAddUserToOrg = () => {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ userId, orgId, role }: { userId: string; orgId: string; role: OrgRoleId }) =>
      AdminUsersService.addToOrg(userId, orgId, role),
    onSuccess: (_data, vars) => {
      qc.invalidateQueries({ queryKey: queryKeys.adminUsers.detail(vars.userId) });
      qc.invalidateQueries({ queryKey: queryKeys.adminUsers.all });
      qc.invalidateQueries({ queryKey: queryKeys.adminOrgs.detail(vars.orgId) });
      toast.success("User added to organization");
    },
    onError: (err) => toast.error(errMessage(err, "Failed to add user to organization"))
  });
};

export const useUpdateUserOrgRole = () => {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ userId, orgId, role }: { userId: string; orgId: string; role: OrgRoleId }) =>
      AdminUsersService.updateRole(userId, orgId, role),
    onSuccess: (_data, vars) => {
      qc.invalidateQueries({ queryKey: queryKeys.adminUsers.detail(vars.userId) });
      qc.invalidateQueries({ queryKey: queryKeys.adminOrgs.detail(vars.orgId) });
      toast.success("Role updated");
    },
    onError: (err) => toast.error(errMessage(err, "Failed to update role"))
  });
};

export const useRemoveUserFromOrg = () => {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ userId, orgId }: { userId: string; orgId: string }) =>
      AdminUsersService.removeFromOrg(userId, orgId),
    onSuccess: (_data, vars) => {
      qc.invalidateQueries({ queryKey: queryKeys.adminUsers.detail(vars.userId) });
      qc.invalidateQueries({ queryKey: queryKeys.adminOrgs.detail(vars.orgId) });
      toast.success("Removed from organization");
    },
    onError: (err) => toast.error(errMessage(err, "Failed to remove from organization"))
  });
};
