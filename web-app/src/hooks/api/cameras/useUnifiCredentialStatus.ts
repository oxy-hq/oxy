import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { LOCAL_WORKSPACE_ID } from "@/libs/utils/constants";
import { CameraService, type UnifiCredentialStatus } from "@/services/api/cameras";
import queryKeys from "../queryKey";

/**
 * Read the workspace's stored UniFi credential status. Drives the
 * "configured ✓" badge in the settings panel and tells dialogs (e.g.
 * EditSiteDialog) whether they should bother asking for the API key
 * inline or just call the credential-aware endpoint directly.
 *
 * Never returns the secret itself — `configured: bool` + timestamps.
 */
export const useUnifiCredentialStatus = (workspaceId: string | undefined) => {
  const effectiveWorkspaceId = workspaceId ?? LOCAL_WORKSPACE_ID;
  return useQuery<UnifiCredentialStatus>({
    queryKey: queryKeys.camera.unifiCredentialStatus(effectiveWorkspaceId),
    queryFn: () => CameraService.getUnifiCredentialStatus(effectiveWorkspaceId)
  });
};

/**
 * Upsert the workspace's UniFi API key. The plaintext goes straight
 * to the server's `org_secrets` via the credential endpoint; we don't
 * retain it in TanStack Query cache.
 */
export const useSetUnifiCredential = (workspaceId: string | undefined) => {
  const effectiveWorkspaceId = workspaceId ?? LOCAL_WORKSPACE_ID;
  const queryClient = useQueryClient();
  return useMutation<UnifiCredentialStatus, Error, { apiKey: string }>({
    mutationFn: ({ apiKey }) => CameraService.setUnifiCredential(effectiveWorkspaceId, apiKey),
    onSuccess: () => {
      queryClient.invalidateQueries({
        queryKey: queryKeys.camera.unifiCredentialStatus(effectiveWorkspaceId)
      });
    }
  });
};

export const useDeleteUnifiCredential = (workspaceId: string | undefined) => {
  const effectiveWorkspaceId = workspaceId ?? LOCAL_WORKSPACE_ID;
  const queryClient = useQueryClient();
  return useMutation<void, Error, void>({
    mutationFn: () => CameraService.deleteUnifiCredential(effectiveWorkspaceId),
    onSuccess: () => {
      queryClient.invalidateQueries({
        queryKey: queryKeys.camera.unifiCredentialStatus(effectiveWorkspaceId)
      });
    }
  });
};
