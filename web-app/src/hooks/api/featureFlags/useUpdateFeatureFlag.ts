import { useMutation, useQueryClient } from "@tanstack/react-query";
import { type FeatureFlag, FeatureFlagsService } from "@/services/api/featureFlags";
import queryKeys from "../queryKey";

interface UpdateInput {
  key: string;
  enabled: boolean;
}

export const useUpdateFeatureFlag = () => {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ key, enabled }: UpdateInput) => FeatureFlagsService.update(key, enabled),
    onMutate: async ({ key, enabled }) => {
      await qc.cancelQueries({ queryKey: queryKeys.featureFlags.list() });
      const previous = qc.getQueryData<FeatureFlag[]>(queryKeys.featureFlags.list());
      if (previous) {
        qc.setQueryData<FeatureFlag[]>(
          queryKeys.featureFlags.list(),
          previous.map((flag) => (flag.key === key ? { ...flag, enabled } : flag))
        );
      }
      return { previous };
    },
    onError: (_err, _vars, context) => {
      if (context?.previous) {
        qc.setQueryData(queryKeys.featureFlags.list(), context.previous);
      }
    },
    onSettled: () => {
      qc.invalidateQueries({ queryKey: queryKeys.featureFlags.list() });
    }
  });
};
