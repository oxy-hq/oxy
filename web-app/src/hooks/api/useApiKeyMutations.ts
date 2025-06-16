import { useMutation, useQueryClient } from "@tanstack/react-query";
import { ApiKeyService } from "@/services/apiKeyService";
import { CreateApiKeyRequest, CreateApiKeyResponse } from "@/types/apiKey";
import queryKeys from "./queryKey";
import { toast } from "sonner";

export const useCreateApiKey = () => {
  const queryClient = useQueryClient();

  return useMutation<CreateApiKeyResponse, Error, CreateApiKeyRequest>({
    mutationFn: ApiKeyService.createApiKey,
    onSuccess: () => {
      // Invalidate and refetch API keys list
      queryClient.invalidateQueries({ queryKey: queryKeys.apiKey.list() });
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

  return useMutation<void, Error, string>({
    mutationFn: ApiKeyService.revokeApiKey,
    onSuccess: () => {
      // Invalidate and refetch API keys list
      queryClient.invalidateQueries({ queryKey: queryKeys.apiKey.list() });
      toast.success("API key revoked successfully");
    },
    onError: (error) => {
      console.error("Failed to revoke API key:", error);
      toast.error("Failed to revoke API key");
    },
  });
};
