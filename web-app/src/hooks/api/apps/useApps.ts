import { useQuery } from "@tanstack/react-query";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { AppService } from "@/services/api";
import queryKeys from "../queryKey";

type UseAppsOptions = {
  enabled?: boolean;
  refetchOnWindowFocus?: boolean;
  refetchOnMount?: boolean | "always";
  /** When true, only published apps are returned. Used by the left sidebar. */
  publishedOnly?: boolean;
};

export default function useApps(
  enabledOrOptions: boolean | UseAppsOptions = true,
  refetchOnWindowFocus = true,
  refetchOnMount: boolean | "always" = false
) {
  const options: UseAppsOptions =
    typeof enabledOrOptions === "boolean"
      ? { enabled: enabledOrOptions, refetchOnWindowFocus, refetchOnMount }
      : enabledOrOptions;
  const {
    enabled = true,
    refetchOnWindowFocus: refetchFocus = true,
    refetchOnMount: refetchMount = false,
    publishedOnly = false
  } = options;
  const { project, branchName } = useCurrentProjectBranch();

  return useQuery({
    queryKey: queryKeys.app.list(project.id, branchName, publishedOnly),
    queryFn: () => AppService.listApps(project.id, branchName, { publishedOnly }),
    enabled,
    refetchOnWindowFocus: refetchFocus,
    refetchOnMount: refetchMount
  });
}
