import { useMutation, useQueryClient } from "@tanstack/react-query";
import { ApiKeyService } from "@/services/api/apiKey";
import { CreateApiKeyRequest, CreateApiKeyResponse } from "@/types/apiKey";
import queryKeys from "../queryKey";
import { toast } from "sonner";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";

export const useCreateApiKey = () => {
  const { project } = useCurrentProjectBranch();
  const projectId = project.id;
  const queryClient = useQueryClient();

  return useMutation<CreateApiKeyResponse, Error, CreateApiKeyRequest>({
    mutationFn: (data) => ApiKeyService.createApiKey(projectId, data),
    onSuccess: () => {
      queryClient.invalidateQueries({
        queryKey: queryKeys.apiKey.list(projectId),
      });
      toast.success("API key created successfully");
    },
    onError: (error) => {
      console.error("Failed to create API key:", error);
      toast.error("Failed to create API key");
    },
  });
};

export const useRevokeApiKey = () => {
  const queryClient = useQueryClient();
  const { project } = useCurrentProjectBranch();
  const projectId = project.id;

  return useMutation<void, Error, string>({
    mutationFn: (id) => ApiKeyService.revokeApiKey(projectId, id),
    onSuccess: () => {
      // Invalidate and refetch API keys list
      queryClient.invalidateQueries({
        queryKey: queryKeys.apiKey.list(projectId),
      });
      toast.success("API key revoked successfully");
    },
    onError: (error) => {
      console.error("Failed to revoke API key:", error);
      toast.error("Failed to revoke API key");
    },
  });
};
