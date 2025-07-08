import { useMutation, useQueryClient } from "@tanstack/react-query";
import { SecretService } from "@/services/secretService";
import {
  CreateSecretRequest,
  CreateSecretResponse,
  UpdateSecretRequest,
  Secret,
  BulkCreateSecretsRequest,
  BulkCreateSecretsResponse,
} from "@/types/secret";
import queryKeys from "./queryKey";
import { toast } from "sonner";

export const useCreateSecret = () => {
  const queryClient = useQueryClient();

  return useMutation<CreateSecretResponse, Error, CreateSecretRequest>({
    mutationFn: SecretService.createSecret,
    onSuccess: () => {
      // Invalidate and refetch secrets list
      queryClient.invalidateQueries({ queryKey: queryKeys.secret.list() });
      toast.success("Secret created successfully");
    },
    onError: (error) => {
      console.error("Failed to create secret:", error);
      toast.error("Failed to create secret");
    },
  });
};

export const useBulkCreateSecrets = () => {
  const queryClient = useQueryClient();

  return useMutation<
    BulkCreateSecretsResponse,
    Error,
    BulkCreateSecretsRequest
  >({
    mutationFn: SecretService.bulkCreateSecrets,
    onSuccess: (data) => {
      // Invalidate and refetch secrets list
      queryClient.invalidateQueries({ queryKey: queryKeys.secret.list() });

      if (data.failed_secrets.length === 0) {
        toast.success(
          `All ${data.created_secrets.length} secrets created successfully`,
        );
      } else {
        toast.warning(
          `${data.created_secrets.length} secrets created, ${data.failed_secrets.length} failed`,
        );
      }
    },
    onError: (error) => {
      console.error("Failed to create secrets in bulk:", error);
      toast.error("Failed to create secrets");
    },
  });
};

export const useUpdateSecret = () => {
  const queryClient = useQueryClient();

  return useMutation<
    Secret,
    Error,
    { id: string; request: UpdateSecretRequest }
  >({
    mutationFn: ({ id, request }) => SecretService.updateSecret(id, request),
    onSuccess: (data) => {
      // Invalidate and refetch secrets list
      queryClient.invalidateQueries({ queryKey: queryKeys.secret.list() });
      // Update the specific secret in cache
      queryClient.invalidateQueries({
        queryKey: queryKeys.secret.item(data.id),
      });
      toast.success("Secret updated successfully");
    },
    onError: (error) => {
      console.error("Failed to update secret:", error);
      toast.error("Failed to update secret");
    },
  });
};

export const useDeleteSecret = () => {
  const queryClient = useQueryClient();

  return useMutation<void, Error, string>({
    mutationFn: SecretService.deleteSecret,
    onSuccess: () => {
      // Invalidate and refetch secrets list
      queryClient.invalidateQueries({ queryKey: queryKeys.secret.list() });
      toast.success("Secret deleted successfully");
    },
    onError: (error) => {
      console.error("Failed to delete secret:", error);
      toast.error("Failed to delete secret");
    },
  });
};
