import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { TestRunService } from "@/services/api";
import queryKeys from "../queryKey";

export function useTestRuns(pathb64: string, enabled = true) {
  const { project } = useCurrentProjectBranch();

  return useQuery({
    queryKey: queryKeys.testRun.list(project.id, pathb64),
    queryFn: () => TestRunService.listRuns(project.id, pathb64),
    enabled: enabled && !!pathb64
  });
}

export function useTestRunDetail(pathb64: string, runIndex: number | null, enabled = true) {
  const { project } = useCurrentProjectBranch();

  return useQuery({
    queryKey: queryKeys.testRun.detail(project.id, pathb64, runIndex ?? -1),
    queryFn: () => TestRunService.getRun(project.id, pathb64, runIndex!),
    enabled: enabled && !!pathb64 && runIndex !== null
  });
}

export function useCreateTestRun() {
  const { project } = useCurrentProjectBranch();
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: ({
      pathb64,
      name,
      projectRunId
    }: {
      pathb64: string;
      name?: string;
      projectRunId?: string;
    }) => TestRunService.createRun(project.id, pathb64, name, projectRunId),
    onSuccess: (_data, { pathb64 }) => {
      queryClient.invalidateQueries({
        queryKey: queryKeys.testRun.list(project.id, pathb64)
      });
    }
  });
}

export function useDeleteTestRun() {
  const { project } = useCurrentProjectBranch();
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: ({ pathb64, runIndex }: { pathb64: string; runIndex: number }) =>
      TestRunService.deleteRun(project.id, pathb64, runIndex),
    onSuccess: (_data, { pathb64 }) => {
      queryClient.invalidateQueries({
        queryKey: queryKeys.testRun.list(project.id, pathb64)
      });
    }
  });
}

export function useHumanVerdicts(pathb64: string, runIndex: number | null, enabled = true) {
  const { project } = useCurrentProjectBranch();

  return useQuery({
    queryKey: queryKeys.humanVerdict.list(project.id, pathb64, runIndex ?? -1),
    queryFn: () => TestRunService.listHumanVerdicts(project.id, pathb64, runIndex!),
    enabled: enabled && !!pathb64 && runIndex !== null
  });
}

export function useSetHumanVerdict() {
  const { project } = useCurrentProjectBranch();
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: ({
      pathb64,
      runIndex,
      caseIndex,
      verdict
    }: {
      pathb64: string;
      runIndex: number;
      caseIndex: number;
      verdict: string | null;
    }) => TestRunService.setHumanVerdict(project.id, pathb64, runIndex, caseIndex, verdict),
    onSuccess: () => {
      queryClient.invalidateQueries({
        queryKey: queryKeys.humanVerdict.all
      });
      queryClient.invalidateQueries({
        queryKey: queryKeys.testRun.all
      });
    },
    onError: (error) => {
      console.error("Failed to set human verdict:", error);
      toast.error("Failed to save verdict");
    }
  });
}
