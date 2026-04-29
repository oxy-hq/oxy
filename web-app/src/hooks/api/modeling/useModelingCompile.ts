import { useMutation, useQueryClient } from "@tanstack/react-query";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { ModelingService } from "@/services/api/modeling";
import type { CompileOutput, NodeSummary } from "@/types/modeling";
import queryKeys from "../queryKey";

export default function useModelingCompile(modelingProjectName: string) {
  const { project, branchName } = useCurrentProjectBranch();
  const queryClient = useQueryClient();
  const nodesKey = queryKeys.modeling.nodes(project.id, modelingProjectName, branchName);

  return useMutation({
    mutationFn: () => ModelingService.compileProject(project.id, modelingProjectName, branchName),
    onSuccess: (data: CompileOutput) => {
      const compiledMap = new Map(data.nodes.map((n) => [n.unique_id, n.compiled_sql]));
      const existing = queryClient.getQueryData<NodeSummary[]>(nodesKey);
      if (existing) {
        queryClient.setQueryData<NodeSummary[]>(
          nodesKey,
          existing.map((node) => ({
            ...node,
            compiled_sql: compiledMap.get(node.unique_id) ?? node.compiled_sql
          }))
        );
      }
    }
  });
}
