import { useQuery } from "@tanstack/react-query";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { AppService } from "@/services/api";
import queryKeys from "../queryKey";

export default function useAppData(
  appPath64: string,
  enabled = true,
  refetchOnWindowFocus = true,
  refetchOnMount: boolean | "always" = false
) {
  const { project, branchName } = useCurrentProjectBranch();

  return useQuery({
    queryKey: queryKeys.app.getAppData(project.id, branchName, appPath64),
    queryFn: () => AppService.getAppData(project.id, branchName, appPath64),
    enabled,
    refetchOnWindowFocus: refetchOnWindowFocus,
    refetchOnMount
  });
}

/** Fetch the LAST cached app data (no execution) — used as a fallback when the
 *  live `useAppData` fetch fails because the ide is down. Enable it only then. */
export function useAppDataCached(appPath64: string, enabled: boolean) {
  const { project, branchName } = useCurrentProjectBranch();

  return useQuery({
    queryKey: queryKeys.app.getAppDataCached(project.id, branchName, appPath64),
    queryFn: () => AppService.getAppDataCached(project.id, branchName, appPath64),
    enabled,
    refetchOnWindowFocus: false
  });
}

export function useAppDisplays(
  filePath: string,
  enabled = true,
  refetchOnWindowFocus = true,
  refetchOnMount: boolean | "always" = false
) {
  const { project, branchName } = useCurrentProjectBranch();

  return useQuery({
    queryKey: queryKeys.app.getDisplays(project.id, branchName, filePath),
    queryFn: () => AppService.getDisplays(project.id, branchName, filePath),
    enabled,
    refetchOnWindowFocus: refetchOnWindowFocus,
    refetchOnMount
  });
}
