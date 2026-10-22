import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { CustomAppsService } from "@/services/api/customApps";
import queryKeys from "../queryKey";

/** The app's Oxy Functions (active build) + their manifest config. */
export function useAppFunctions(id: string | undefined) {
  return useQuery({
    queryKey: queryKeys.customApps.functions(id ?? ""),
    queryFn: () => CustomAppsService.listFunctions(id as string),
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
    queryKey: queryKeys.customApps.functionInvocations(id ?? "", name ?? ""),
    queryFn: () => CustomAppsService.listFunctionInvocations(id as string, name as string),
    enabled: !!id && !!name && enabled
  });
}

/** A single function-job run: status + persisted logs. Polls every 1.5s while
 *  the run is non-terminal so a just-triggered run is followed to completion. */
export function useFunctionRun(id: string | undefined, runId: string | undefined) {
  return useQuery({
    queryKey: queryKeys.customApps.functionRun(id ?? "", runId ?? ""),
    queryFn: () => CustomAppsService.getFunctionRun(id as string, runId as string),
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

const INVOCATION_CALLS_POLL_MS = 2000;

/** One invocation's `ctx.*` call trace. Enabled only when the row is opened. A
 *  recorded trace never changes, so it is never refetched; a trace still being
 *  written (`reason: "running"`) is polled every 2 s. */
export function useInvocationCalls(
  id: string | undefined,
  invocationId: string | undefined,
  enabled: boolean
) {
  return useQuery({
    queryKey: queryKeys.customApps.invocationCalls(id ?? "", invocationId ?? ""),
    queryFn: () => CustomAppsService.getInvocationCalls(id as string, invocationId as string),
    enabled: !!id && !!invocationId && enabled,
    staleTime: (query) => (query.state.data?.recorded ? Number.POSITIVE_INFINITY : 0),
    refetchInterval: (query) => invocationCallsPollMs(query.state.data?.reason ?? null)
  });
}

/** The poll interval for a trace answered with `reason`: 2 s while the
 *  server says it is still running, otherwise none. The bound is the server's,
 *  on the server's clock: it stops answering `running` past the function
 *  ceiling plus grace (`invocation_calls::RUNNING_WINDOW`), and 15 s after a
 *  run settles, so an orphaned row ends its own polling. A client-side cap
 *  would either stop following a long run (a poll count) or compare the
 *  browser's clock with the server's (an age). */
export function invocationCallsPollMs(reason: string | null): number | false {
  return reason === "running" ? INVOCATION_CALLS_POLL_MS : false;
}

/** Trigger a one-off background run of a function as a job. On success,
 *  invalidates that function's invocation history so the new run appears. */
export function useRunFunction() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({ id, name, input }: { id: string; name: string; input?: unknown }) =>
      CustomAppsService.runFunction(id, name, input),
    onSuccess: (_data, { id, name }) => {
      queryClient.invalidateQueries({
        queryKey: queryKeys.customApps.functionInvocations(id, name)
      });
    }
  });
}
