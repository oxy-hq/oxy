import { useQueryClient } from "@tanstack/react-query";
import { useCallback } from "react";
import queryKeys from "@/hooks/api/queryKey";
import { useEditorContext } from "./contexts/useEditorContext";

export const useEditorQueryInvalidation = () => {
  const queryClient = useQueryClient();
  const { project, branchName, pathb64 } = useEditorContext();

  const invalidateAgentQueries = useCallback(() => {
    queryClient.invalidateQueries({
      queryKey: queryKeys.agent.list(project.id, branchName),
    });
  }, [queryClient, project.id, branchName]);

  const invalidateAppQueries = useCallback(() => {
    queryClient.invalidateQueries({
      queryKey: queryKeys.app.getAppData(project.id, branchName, pathb64),
    });
    queryClient.invalidateQueries({
      queryKey: queryKeys.app.getDisplays(project.id, branchName, pathb64),
    });
  }, [queryClient, project.id, branchName, pathb64]);

  const invalidateFileQueries = useCallback(() => {
    queryClient.invalidateQueries({
      queryKey: queryKeys.file.get(project.id, branchName, pathb64),
    });
  }, [queryClient, project.id, branchName, pathb64]);

  return {
    invalidateAgentQueries,
    invalidateAppQueries,
    invalidateFileQueries,
  };
};
