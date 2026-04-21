import { useMutation, useQueryClient } from "@tanstack/react-query";
import {
  LocalWorkspaceService,
  type SetupDemoResponse,
  type SetupEmptyResponse
} from "@/services/api/localWorkspace";
import queryKeys from "../queryKey";

export const useSetupEmptyWorkspace = (workspaceId: string) => {
  const queryClient = useQueryClient();
  return useMutation<SetupEmptyResponse, Error>({
    mutationKey: queryKeys.workspaces.localSetup(workspaceId),
    mutationFn: () => LocalWorkspaceService.setupEmpty(workspaceId),
    onSuccess: () => {
      queryClient.invalidateQueries({
        queryKey: queryKeys.workspaces.item(workspaceId)
      });
    }
  });
};

export const useSetupDemoWorkspace = (workspaceId: string) => {
  const queryClient = useQueryClient();
  return useMutation<SetupDemoResponse, Error>({
    mutationKey: queryKeys.workspaces.localSetup(workspaceId),
    mutationFn: () => LocalWorkspaceService.setupDemo(workspaceId),
    onSuccess: () => {
      queryClient.invalidateQueries({
        queryKey: queryKeys.workspaces.item(workspaceId)
      });
    }
  });
};
