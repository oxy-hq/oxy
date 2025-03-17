import { useQuery } from "@tanstack/react-query";
import queryKeys from "./queryKey";
import { apiClient } from "@/services/axios";

type WorkflowItem = {
  name: string;
  path: string;
};

const fetchWorkflows = async () => {
  const { data } = await apiClient.get("/workflows");
  return data as WorkflowItem[];
};

const useWorkflows = () => {
  return useQuery({
    queryKey: queryKeys.workflow.list(),
    queryFn: fetchWorkflows,
  });
};

export default useWorkflows;
