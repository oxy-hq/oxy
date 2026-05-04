import { useQuery } from "@tanstack/react-query";
import { FeatureFlagsService } from "@/services/api/featureFlags";
import queryKeys from "../queryKey";

export const useFeatureFlags = () =>
  useQuery({
    queryKey: queryKeys.featureFlags.list(),
    queryFn: () => FeatureFlagsService.list()
  });
