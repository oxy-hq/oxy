import { SemanticService, SemanticQueryRequest } from "@/services/api/semantic";
import { useMutation, useQuery } from "@tanstack/react-query";
import useCurrentProjectBranch from "../useCurrentProjectBranch";

export function useExecuteSemanticQuery() {
  const { project } = useCurrentProjectBranch();
  const projectId = project.id;

  return useMutation({
    mutationFn: (request: SemanticQueryRequest) =>
      SemanticService.executeSemanticQuery(projectId, request),
  });
}

export function useCompileSemanticQuery() {
  const { project } = useCurrentProjectBranch();
  const projectId = project.id;

  return useMutation({
    mutationFn: (request: SemanticQueryRequest) =>
      SemanticService.compileSemanticQuery(projectId, request),
  });
}

export function useTopicDetails(topicName: string | undefined) {
  const { project } = useCurrentProjectBranch();
  const projectId = project.id;

  return useQuery({
    queryKey: ["topicDetails", projectId, topicName],
    queryFn: () => {
      if (!topicName) throw new Error("Topic name is required");
      return SemanticService.getTopicDetails(projectId, topicName);
    },
    enabled: !!topicName,
  });
}

export function useViewDetails(viewName: string | undefined) {
  const { project } = useCurrentProjectBranch();
  const projectId = project.id;

  return useQuery({
    queryKey: ["viewDetails", projectId, viewName],
    queryFn: () => {
      if (!viewName) throw new Error("View name is required");
      return SemanticService.getViewDetails(projectId, viewName);
    },
    enabled: !!viewName,
  });
}
