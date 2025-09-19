import { useQuery } from "@tanstack/react-query";
import queryKeys from "../queryKey";
import { AppService } from "@/services/api";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";

export default function useAppData(
  appPath64: string,
  enabled = true,
  refetchOnWindowFocus = true,
  refetchOnMount: boolean | "always" = false,
) {
  const { project, branchName } = useCurrentProjectBranch();

  return useQuery({
    queryKey: queryKeys.app.getAppData(project.id, branchName, appPath64),
    queryFn: () => AppService.getAppData(project.id, branchName, appPath64),
    enabled,
    refetchOnWindowFocus: refetchOnWindowFocus,
    refetchOnMount,
  });
}

export function useData(
  filePath: string,
  enabled = true,
  refetchOnWindowFocus = true,
  refetchOnMount: boolean | "always" = false,
) {
  const { project, branchName } = useCurrentProjectBranch();

  return useQuery({
    queryKey: queryKeys.app.getData(project.id, branchName, filePath),
    queryFn: () => AppService.getData(project.id, branchName, filePath),
    enabled,
    refetchOnWindowFocus: refetchOnWindowFocus,
    refetchOnMount,
  });
}

export function useAppDisplays(
  filePath: string,
  enabled = true,
  refetchOnWindowFocus = true,
  refetchOnMount: boolean | "always" = false,
) {
  const { project, branchName } = useCurrentProjectBranch();

  return useQuery({
    queryKey: queryKeys.app.getDisplays(project.id, branchName, filePath),
    queryFn: () => AppService.getDisplays(project.id, branchName, filePath),
    enabled,
    refetchOnWindowFocus: refetchOnWindowFocus,
    refetchOnMount,
  });
}
