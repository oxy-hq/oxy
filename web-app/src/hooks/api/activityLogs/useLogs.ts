import { useQuery } from "@tanstack/react-query";
import { LogsService } from "@/services/api/logs";

export const useLogs = () => {
  return useQuery({
    queryKey: ["logs"],
    queryFn: LogsService.getLogs,
    staleTime: 5 * 60 * 1000,
    enabled: true,
  });
};
