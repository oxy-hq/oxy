import { useQuery } from "@tanstack/react-query";
import { UserService } from "@/services/api";
import type { UserListResponse } from "@/types/auth";
import queryKeys from "../queryKey";

const useUsers = (
  workspaceId: string,
  enabled = true,
  refetchOnWindowFocus = true,
  refetchOnMount: boolean | "always" = false
) =>
  useQuery<UserListResponse, Error>({
    queryKey: queryKeys.user.list(workspaceId),
    queryFn: () => UserService.getUsers(workspaceId),
    enabled,
    refetchOnWindowFocus,
    refetchOnMount,
    retry: (failureCount, error) => {
      // Don't retry for unauthorized errors
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
