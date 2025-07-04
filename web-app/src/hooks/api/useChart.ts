import { useQuery } from "@tanstack/react-query";

import queryKeys from "./queryKey";
import { ChartService } from "@/services/api";

export default function useChart(file_path: string, enabled = true) {
  return useQuery({
    queryKey: queryKeys.chart.get(file_path),
    queryFn: () => ChartService.getChart(file_path),
    enabled,
  });
}
