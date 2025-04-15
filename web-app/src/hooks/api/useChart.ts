import { useQuery } from "@tanstack/react-query";

import queryKeys from "./queryKey";
import { service } from "@/services/service";

export default function useChart(file_path: string, enabled = true) {
  return useQuery({
    queryKey: queryKeys.chart.get(file_path),
    queryFn: () => service.getChart(file_path),
    enabled,
  });
}
