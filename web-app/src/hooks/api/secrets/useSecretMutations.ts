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
import queryKeys from "../queryKey";
import { toast } from "sonner";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";

export const useCreateSecret = () => {
  const queryClient = useQueryClient();
  const { project } = useCurrentProjectBranch();
  const projectId = project.id;

  return useMutation<CreateSecretResponse, Error, CreateSecretRequest>({
    mutationFn: (request) => SecretService.createSecret(projectId, request),
    onSuccess: () => {
      // Invalidate and refetch secrets list
      queryClient.invalidateQueries({
        queryKey: queryKeys.secret.list(projectId),
      });
      toast.success("Secret created successfully");
    },
    onError: (error) => {
      console.error("Failed to create secret:", error);
      toast.error("Failed to create secret");
    },
  });
};

export const useBulkCreateSecrets = (projectId: string) => {
  const queryClient = useQueryClient();

  return useMutation<
    BulkCreateSecretsResponse,
    Error,
    BulkCreateSecretsRequest
  >({
    mutationFn: (request) =>
      SecretService.bulkCreateSecrets(projectId, request),
    onSuccess: (data) => {
      // Invalidate and refetch secrets list
      queryClient.invalidateQueries({
        queryKey: queryKeys.secret.list(projectId),
      });

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
  const { project } = useCurrentProjectBranch();
  const projectId = project.id;

  return useMutation<
    Secret,
    Error,
    { id: string; request: UpdateSecretRequest }
  >({
    mutationFn: ({ id, request }) =>
      SecretService.updateSecret(projectId, id, request),
    onSuccess: (data) => {
      // Invalidate and refetch secrets list
      queryClient.invalidateQueries({
        queryKey: queryKeys.secret.list(projectId),
      });
      // Update the specific secret in cache
      queryClient.invalidateQueries({
        queryKey: queryKeys.secret.item(projectId, data.id),
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
  const { project } = useCurrentProjectBranch();
  const projectId = project.id;

  return useMutation<void, Error, string>({
    mutationFn: (id) => SecretService.deleteSecret(projectId, id),
    onSuccess: () => {
      // Invalidate and refetch secrets list
      queryClient.invalidateQueries({
        queryKey: queryKeys.secret.list(projectId),
      });
      toast.success("Secret deleted successfully");
    },
    onError: (error) => {
      console.error("Failed to delete secret:", error);
      toast.error("Failed to delete secret");
    },
  });
};
