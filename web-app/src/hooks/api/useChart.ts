import { useQuery } from "@tanstack/react-query";
import queryKeys from "./queryKey";
import { ChartService } from "@/services/api";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";

export default function useChart(file_path: string, enabled = true) {
  const { project, branchName } = useCurrentProjectBranch();

  return useQuery({
    queryKey: queryKeys.chart.get(project.id, branchName, file_path),
    queryFn: () => ChartService.getChart(project.id, branchName, file_path),
    enabled,
  });
}
