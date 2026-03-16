import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { TestProjectRunService } from "@/services/api";
import queryKeys from "../queryKey";

export function useTestProjectRuns() {
  const { project } = useCurrentProjectBranch();

  return useQuery({
    queryKey: queryKeys.testProjectRun.list(project.id),
    queryFn: () => TestProjectRunService.listProjectRuns(project.id)
  });
}

export function useCreateTestProjectRun() {
  const { project } = useCurrentProjectBranch();
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: ({ name }: { name?: string }) =>
      TestProjectRunService.createProjectRun(project.id, name),
    onSuccess: () => {
      queryClient.invalidateQueries({
        queryKey: queryKeys.testProjectRun.list(project.id)
      });
    }
  });
}

export function useDeleteTestProjectRun() {
  const { project } = useCurrentProjectBranch();
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: ({ projectRunId }: { projectRunId: string }) =>
      TestProjectRunService.deleteProjectRun(project.id, projectRunId),
    onSuccess: () => {
      queryClient.invalidateQueries({
        queryKey: queryKeys.testProjectRun.list(project.id)
      });
    }
  });
}
