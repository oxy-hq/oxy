import { useQuery } from "@tanstack/react-query";
import { LogsService } from "@/services/api/logs";
import queryKeys from "../queryKey";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";

export const useLogs = () => {
  const { project } = useCurrentProjectBranch();

  return useQuery({
    queryKey: queryKeys.logs.list(project.id),
    queryFn: () => LogsService.getLogs(project.id),
    staleTime: 5 * 60 * 1000,
    enabled: true,
  });
};
