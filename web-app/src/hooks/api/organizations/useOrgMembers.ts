import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { OrganizationService } from "@/services/api/organizations";
import queryKeys from "../queryKey";

export const useOrgMembers = (orgId: string, enabled = true) => {
  return useQuery({
    queryKey: queryKeys.org.members(orgId),
    queryFn: () => OrganizationService.listMembers(orgId),
    enabled: enabled && !!orgId
  });
};

export const useUpdateMemberRole = () => {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({ orgId, userId, role }: { orgId: string; userId: string; role: string }) =>
      OrganizationService.updateMemberRole(orgId, userId, role),
    onSuccess: (_data, variables) => {
      queryClient.invalidateQueries({ queryKey: queryKeys.org.members(variables.orgId) });
    }
  });
};

export const useRemoveMember = () => {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({ orgId, userId }: { orgId: string; userId: string }) =>
      OrganizationService.removeMember(orgId, userId),
    onSuccess: (_data, variables) => {
      queryClient.invalidateQueries({ queryKey: queryKeys.org.members(variables.orgId) });
    }
  });
};
