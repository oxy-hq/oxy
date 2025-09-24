import { useMutation, useQueryClient } from "@tanstack/react-query";
import { UserService } from "@/services/api";
import queryKeys from "../queryKey";

export const useUpdateUserRole = (workspaceId: string) => {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: ({ userId, role }: { userId: string; role: string }) =>
      UserService.updateUserRole(workspaceId, userId, role),
    onSuccess: () => {
      queryClient.invalidateQueries({
        queryKey: queryKeys.user.list(workspaceId),
      });
    },
  });
};

export const useRemoveUser = (workspaceId: string) => {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (userId: string) => UserService.removeUser(workspaceId, userId),
    onSuccess: () => {
      queryClient.invalidateQueries({
        queryKey: queryKeys.user.list(workspaceId),
      });
    },
  });
};

export const useAddUserToWorkspace = () => {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: ({
      workspaceId,
      email,
      role,
    }: {
      workspaceId: string;
      email: string;
      role: string;
    }) => UserService.addUserToWorkspace(workspaceId, email, role),
    onSuccess: (_data, variables) => {
      queryClient.invalidateQueries({
        queryKey: queryKeys.user.list(variables.workspaceId),
      });
    },
  });
};
