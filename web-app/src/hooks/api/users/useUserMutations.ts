import { useMutation, useQueryClient } from "@tanstack/react-query";
import { UserService } from "@/services/api";
import queryKeys from "../queryKey";

export const useUpdateUserRole = (organizationId: string) => {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: ({ userId, role }: { userId: string; role: string }) =>
      UserService.updateUserRole(organizationId, userId, role),
    onSuccess: () => {
      queryClient.invalidateQueries({
        queryKey: queryKeys.user.list(organizationId),
      });
    },
  });
};

export const useRemoveUser = (organizationId: string) => {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (userId: string) =>
      UserService.removeUser(organizationId, userId),
    onSuccess: () => {
      queryClient.invalidateQueries({
        queryKey: queryKeys.user.list(organizationId),
      });
    },
  });
};

export const useAddUserToOrg = () => {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: ({
      organizationId,
      email,
      role,
    }: {
      organizationId: string;
      email: string;
      role: string;
    }) => UserService.addUserToOrganization(organizationId, email, role),
    onSuccess: (_data, variables) => {
      queryClient.invalidateQueries({
        queryKey: queryKeys.user.list(variables.organizationId),
      });
    },
  });
};
