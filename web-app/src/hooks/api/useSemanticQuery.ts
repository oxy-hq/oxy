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

export function useTopicDetails(filePathB64: string | undefined) {
  const { project } = useCurrentProjectBranch();
  const projectId = project.id;

  return useQuery({
    queryKey: ["topicDetails", projectId, filePathB64],
    queryFn: () => {
      if (!filePathB64) throw new Error("Topic file path is required");
      return SemanticService.getTopicDetails(projectId, filePathB64);
    },
    enabled: !!filePathB64,
    retry: false,
  });
}

export function useViewDetails(filePathB64: string | undefined) {
  const { project } = useCurrentProjectBranch();
  const projectId = project.id;

  return useQuery({
    queryKey: ["viewDetails", projectId, filePathB64],
    queryFn: () => {
      if (!filePathB64) throw new Error("View file path is required");
      return SemanticService.getViewDetails(projectId, filePathB64);
    },
    enabled: !!filePathB64,
    retry: false,
  });
}
