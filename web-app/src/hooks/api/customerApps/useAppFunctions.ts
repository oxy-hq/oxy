import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { CustomerAppsService } from "@/services/api/customerApps";
import queryKeys from "../queryKey";

/** The app's Oxy Functions (active build) + their manifest config. */
export function useAppFunctions(id: string | undefined) {
  return useQuery({
    queryKey: queryKeys.customerApps.functions(id ?? ""),
    queryFn: () => CustomerAppsService.listFunctions(id as string),
    enabled: !!id
  });
}

/** Recent invocation history for one function. Enabled only when opened. */
export function useFunctionInvocations(
  id: string | undefined,
  name: string | undefined,
  enabled: boolean
) {
  return useQuery({
    queryKey: queryKeys.customerApps.functionInvocations(id ?? "", name ?? ""),
    queryFn: () => CustomerAppsService.listFunctionInvocations(id as string, name as string),
    enabled: !!id && !!name && enabled
  });
}

/** A single function-job run: status + persisted logs. Polls every 1.5s while
 *  the run is non-terminal so a just-triggered run is followed to completion. */
export function useFunctionRun(id: string | undefined, runId: string | undefined) {
  return useQuery({
    queryKey: queryKeys.customerApps.functionRun(id ?? "", runId ?? ""),
    queryFn: () => CustomerAppsService.getFunctionRun(id as string, runId as string),
    enabled: !!id && !!runId,
    refetchInterval: (query) => {
      const status = query.state.data?.status;
      // Poll while queued (awaiting a worker) or running; stop on any terminal
      // state — including timed_out, or the panel would spin forever.
      const terminal =
        status === "done" ||
        status === "failed" ||
        status === "cancelled" ||
        status === "timed_out";
      return terminal ? false : 1500;
    }
  });
}

/** Trigger a one-off background run of a function as a job. On success,
 *  invalidates that function's invocation history so the new run appears. */
export function useRunFunction() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({ id, name, input }: { id: string; name: string; input?: unknown }) =>
      CustomerAppsService.runFunction(id, name, input),
    onSuccess: (_data, { id, name }) => {
      queryClient.invalidateQueries({
        queryKey: queryKeys.customerApps.functionInvocations(id, name)
      });
    }
  });
}
