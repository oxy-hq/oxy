import { useQuery } from "@tanstack/react-query";
import { UserService } from "@/services/api";
import type { UserListResponse } from "@/types/auth";
import queryKeys from "../queryKey";

const useUsers = (
  enabled = true,
  refetchOnWindowFocus = true,
  refetchOnMount: boolean | "always" = false
) =>
  useQuery<UserListResponse, Error>({
    queryKey: queryKeys.user.list(),
    queryFn: () => UserService.getAllUsers(),
    enabled,
    refetchOnWindowFocus,
    refetchOnMount,
    retry: (failureCount, error) => {
      if (
        error.message.includes("401") ||
        error.message.includes("403") ||
        error.message.includes("Unauthorized")
      ) {
        return false;
      }
      return failureCount < 3;
    }
  });

export default useUsers;
