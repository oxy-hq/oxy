import { useMutation, useQueryClient } from "@tanstack/react-query";
import { service } from "@/services/service";
import queryKeys from "./queryKey";
import { toast } from "sonner";

export const useUpdateUserRole = () => {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: ({ userId, role }: { userId: string; role: string }) =>
      service.updateUserRole(userId, role),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: queryKeys.user.list() });
      toast.success("User role updated successfully");
    },
    onError: (
      error: Error & { response?: { status?: number }; status?: number },
    ) => {
      const errorCode = error?.response?.status || error?.status || "Unknown";
      toast.error(`Operation failed (Error ${errorCode})`);
    },
  });
};

export const useDeleteUser = () => {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (userId: string) => service.deleteUser(userId),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: queryKeys.user.list() });
      toast.success("User deleted successfully");
    },
    onError: (
      error: Error & { response?: { status?: number }; status?: number },
    ) => {
      const errorCode = error?.response?.status || error?.status || "Unknown";
      toast.error(`Operation failed (Error ${errorCode})`);
    },
  });
};

export const useUpdateUser = () => {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: ({ userId, status }: { userId: string; status: string }) =>
      service.updateUser(userId, { status }),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: queryKeys.user.list() });
      toast.success("User updated successfully");
    },
    onError: (
      error: Error & { response?: { status?: number }; status?: number },
    ) => {
      const errorCode = error?.response?.status || error?.status || "Unknown";
      toast.error(`Operation failed (Error ${errorCode})`);
    },
  });
};
