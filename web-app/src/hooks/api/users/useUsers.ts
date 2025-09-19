import { UserService } from "@/services/api";
import { useQuery } from "@tanstack/react-query";
import queryKeys from "../queryKey";
import { UserListResponse } from "@/types/auth";

const useUsers = (
  organizationId: string,
  enabled = true,
  refetchOnWindowFocus = true,
  refetchOnMount: boolean | "always" = false,
) =>
  useQuery<UserListResponse, Error>({
    queryKey: queryKeys.user.list(organizationId),
    queryFn: () => UserService.getUsers(organizationId),
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
    },
  });

export default useUsers;
