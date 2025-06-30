import { useQuery } from "@tanstack/react-query";
import { service } from "@/services/service";
import queryKeys from "./queryKey";

const useCurrentUser = () => {
  return useQuery({
    queryKey: queryKeys.user.current(),
    queryFn: () => service.getCurrentUser(),
    staleTime: 5 * 60 * 1000, // 5 minutes
    retry: false,
  });
};

export default useCurrentUser;
