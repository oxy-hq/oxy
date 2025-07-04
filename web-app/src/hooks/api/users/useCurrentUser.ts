import { useQuery } from "@tanstack/react-query";
import { UserService } from "@/services/api";
import queryKeys from "../queryKey";

const useCurrentUser = () => {
  return useQuery({
    queryKey: queryKeys.user.current(),
    queryFn: () => UserService.getCurrentUser(),
    staleTime: 5 * 60 * 1000, // 5 minutes
    retry: false,
  });
};

export default useCurrentUser;
