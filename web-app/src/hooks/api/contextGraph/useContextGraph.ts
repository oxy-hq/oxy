import { useQuery } from "@tanstack/react-query";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { ContextGraphService } from "@/services/api/contextGraph";
import queryKeys from "../queryKey";

const useContextGraph = () => {
  const { project, branchName } = useCurrentProjectBranch();

  return useQuery({
    queryKey: queryKeys.contextGraph.graph(project.id, branchName),
    queryFn: () => ContextGraphService.getContextGraph(project.id, branchName)
  });
};

export default useContextGraph;
