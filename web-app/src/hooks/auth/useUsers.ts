import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import queryKeys from "@/hooks/api/queryKey";
import { UserService } from "@/services/api";

export const useAllUsers = () => {
  return useQuery({
    queryKey: queryKeys.user.list(),
    queryFn: () => UserService.getAllUsers()
  });
};

export const useUpdateUserRole = () => {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({ userId, role }: { userId: string; role: string }) =>
      UserService.updateUser(userId, { role }),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: queryKeys.user.list() });
    }
  });
};

export const useRemoveUser = () => {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (userId: string) => UserService.deleteUser(userId),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: queryKeys.user.list() });
    }
  });
};
