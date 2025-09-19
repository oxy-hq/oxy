import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { OrganizationService } from "@/services/api";
import queryKeys from "../queryKey";
import { CreateOrganizationRequest } from "@/types/organization";

export function useOrganizations() {
  return useQuery({
    queryKey: queryKeys.organizations.list(),
    queryFn: OrganizationService.listOrganizations,
  });
}

export function useCreateOrganization() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (data: CreateOrganizationRequest) =>
      OrganizationService.createOrganization(data),
    onSuccess: () => {
      // Invalidate and refetch organizations list
      queryClient.invalidateQueries({ queryKey: queryKeys.organizations.list() });
    },
  });
}
