import { useMutation, useQueryClient } from "@tanstack/react-query";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { AppService } from "@/services/api";
import queryKeys from "../queryKey";

export default function usePublishApp() {
  const queryClient = useQueryClient();
  const { project, branchName } = useCurrentProjectBranch();

  return useMutation({
    mutationFn: ({ pathb64, publish }: { pathb64: string; publish: boolean }) =>
      publish
        ? AppService.publishApp(project.id, branchName, pathb64)
        : AppService.unpublishApp(project.id, branchName, pathb64),
    onSuccess: (_data, { pathb64 }) => {
      // App list (sidebar + IDE Objects pill).
      queryClient.invalidateQueries({ queryKey: queryKeys.app.all });
      // File content the Monaco editor reads — the YAML now has a new `published`
      // line, so refetch so the open buffer reflects what's on disk.
      queryClient.invalidateQueries({
        queryKey: queryKeys.file.get(project.id, branchName, pathb64)
      });
    }
  });
}
