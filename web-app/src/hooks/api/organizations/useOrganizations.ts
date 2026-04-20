import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { OrganizationService } from "@/services/api/organizations";
import queryKeys from "../queryKey";

export const useOrgs = () => {
  return useQuery({
    queryKey: queryKeys.org.list(),
    queryFn: () => OrganizationService.listOrgs()
  });
};

export const useOrg = (orgId: string, enabled = true) => {
  return useQuery({
    queryKey: queryKeys.org.item(orgId),
    queryFn: () => OrganizationService.getOrg(orgId),
    enabled: enabled && !!orgId
  });
};

export const useCreateOrg = () => {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (data: { name: string; slug: string }) => OrganizationService.createOrg(data),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: queryKeys.org.list() });
    }
  });
};

export const useUpdateOrg = () => {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({ orgId, data }: { orgId: string; data: { name?: string; slug?: string } }) =>
      OrganizationService.updateOrg(orgId, data),
    onSuccess: (_data, variables) => {
      queryClient.invalidateQueries({ queryKey: queryKeys.org.item(variables.orgId) });
      queryClient.invalidateQueries({ queryKey: queryKeys.org.list() });
    }
  });
};

export const useDeleteOrg = () => {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (orgId: string) => OrganizationService.deleteOrg(orgId),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: queryKeys.org.list() });
    }
  });
};
