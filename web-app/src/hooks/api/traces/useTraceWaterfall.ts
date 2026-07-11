import { useQuery } from "@tanstack/react-query";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { TracesService, type WaterfallResponse } from "@/services/api/traces";
import queryKeys from "../queryKey";

/**
 * Trace waterfall summary (span/error/llm/tool counts + total tokens + wall
 * time). Backs the Trace Detail summary strip. Distinct from `useTraceDetail`,
 * which returns the raw span tree used to render the waterfall itself.
 */
const useTraceWaterfall = (traceId: string, enabled = true) => {
  const { project } = useCurrentProjectBranch();
  const projectId = project.id;

  return useQuery<WaterfallResponse, Error>({
    queryKey: queryKeys.trace.waterfall(projectId, traceId),
    queryFn: () => TracesService.getTraceWaterfall(projectId, traceId),
    enabled: enabled && !!traceId
  });
};

export default useTraceWaterfall;
