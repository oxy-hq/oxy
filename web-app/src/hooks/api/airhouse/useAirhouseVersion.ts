import { useQuery } from "@tanstack/react-query";
import { AirhouseService } from "@/services/api";
import queryKeys from "../queryKey";

/**
 * The running Airhouse deployment's software version. Global (no workspace
 * scope), cached for 10 minutes, and never retried — the caller hides the
 * version badge on any failure (unconfigured 503 / upstream 502), so a retry
 * buys nothing.
 */
const useAirhouseVersion = () => {
  return useQuery({
    queryKey: queryKeys.airhouse.version(),
    queryFn: () => AirhouseService.getVersion(),
    retry: false,
    staleTime: 10 * 60 * 1000
  });
};

export default useAirhouseVersion;
