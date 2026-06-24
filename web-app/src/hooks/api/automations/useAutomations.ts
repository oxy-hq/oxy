import { useQuery } from "@tanstack/react-query";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { AutomationService } from "@/services/api";
import queryKeys from "../queryKey";

const useAutomations = () => {
  const { project, branchName } = useCurrentProjectBranch();

  return useQuery({
    queryKey: queryKeys.automation.list(project.id, branchName),
    queryFn: () => AutomationService.listAutomations(project.id, branchName)
  });
};

export default useAutomations;
