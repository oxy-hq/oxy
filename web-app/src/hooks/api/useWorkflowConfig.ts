import { useQuery } from "@tanstack/react-query";
import queryKeys from "./queryKey";
import { apiClient } from "@/services/axios";
import { WorkflowConfig } from "@/stores/useWorkflow.ts";

const fetchWorkflow = async (relative_path: string) => {
  const pathb64 = btoa(relative_path);
  const { data } = await apiClient.get(
    `/workflows/${encodeURIComponent(pathb64)}`,
  );
  return data.data as WorkflowConfig;
};

const useWorkflowConfig = (relative_path: string) => {
  return useQuery({
    queryKey: queryKeys.workflow.get(relative_path),
    queryFn: () => fetchWorkflow(relative_path),
    enabled: true,
  });
};

export default useWorkflowConfig;
