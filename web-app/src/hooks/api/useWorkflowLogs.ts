import { useQuery } from "@tanstack/react-query";
import queryKeys from "./queryKey";
import { apiClient } from "@/services/axios";
import { LogItem } from "./runWorkflow";

const fetchWorkflowLogs = async (relativePath: string) => {
  const pathb64 = btoa(relativePath);
  const { data } = await apiClient.get(
    `/workflows/${encodeURIComponent(pathb64)}/logs`,
  );
  return data.logs as LogItem[];
};

const useWorkflowLogs = (relativePath: string) => {
  return useQuery({
    queryKey: queryKeys.workflow.getLogs(relativePath),
    queryFn: () => fetchWorkflowLogs(relativePath),
  });
};

export default useWorkflowLogs;
