import { useMutation } from "@tanstack/react-query";
import { AnalyticsService } from "@/services/api/analytics";

interface RevertVariables {
  runId: string;
  /** Files to revert; omit / empty to revert every builder-changed file. */
  filePaths?: string[];
}

/**
 * Reverts builder-applied file change(s) for a run. `projectId` is passed
 * explicitly because the analytics thread page is not inside the IDE route
 * (so `useCurrentProjectBranch` is unavailable).
 */
export default function useRevertBuilderFileChanges(projectId: string) {
  return useMutation({
    mutationFn: ({ runId, filePaths }: RevertVariables) =>
      AnalyticsService.revertBuilderFileChanges(projectId, runId, filePaths)
  });
}
