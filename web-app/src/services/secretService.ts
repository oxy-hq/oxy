import {
  Secret,
  SecretListResponse,
  CreateSecretRequest,
  CreateSecretResponse,
  UpdateSecretRequest,
  BulkCreateSecretsRequest,
  BulkCreateSecretsResponse,
} from "@/types/secret";
import { apiClient } from "./api/axios";

const BASE_PATH = "/secrets";

export class SecretService {
  /**
   * Create a new secret
   */
  static async createSecret(
    request: CreateSecretRequest,
  ): Promise<CreateSecretResponse> {
    const response = await apiClient.post<CreateSecretResponse>(
      BASE_PATH,
      request,
    );
    return response.data;
  }

  /**
   * Create multiple secrets in bulk
   */
  static async bulkCreateSecrets(
    request: BulkCreateSecretsRequest,
  ): Promise<BulkCreateSecretsResponse> {
    const response = await apiClient.post<BulkCreateSecretsResponse>(
      `${BASE_PATH}/bulk`,
      request,
    );
    return response.data;
  }

  /**
   * List all secrets for the current user
   */
  static async listSecrets(): Promise<SecretListResponse> {
    const response = await apiClient.get<SecretListResponse>(BASE_PATH);
    return response.data;
  }

  /**
   * Get details of a specific secret (metadata only, no value)
   */
  static async getSecret(id: string): Promise<Secret> {
    const response = await apiClient.get<Secret>(`${BASE_PATH}/${id}`);
    return response.data;
  }

  /**
   * Update an existing secret
   */
  static async updateSecret(
    id: string,
    request: UpdateSecretRequest,
  ): Promise<Secret> {
    const response = await apiClient.put<Secret>(`${BASE_PATH}/${id}`, request);
    return response.data;
  }

  /**
   * Delete a secret
   */
  static async deleteSecret(id: string): Promise<void> {
    await apiClient.delete(`${BASE_PATH}/${id}`);
  }

  /**
   * Mask a secret value for safe display
   */
  static maskSecret(value: string): string {
    if (!value || value.length === 0) {
      return "";
    }

    if (value.length <= 8) {
      return "*".repeat(value.length);
    }

    // Show first 4 and last 4 characters for longer secrets
    const start = value.substring(0, 4);
    const end = value.substring(value.length - 4);
    const middle = "*".repeat(Math.max(0, value.length - 8));

    return `${start}${middle}${end}`;
  }

  /**
   * Validate secret name
   */
  static validateSecretName(name: string): string | null {
    if (!name || name.trim().length === 0) {
      return "Secret name is required";
    }

    if (name.length > 255) {
      return "Secret name must be less than 255 characters";
    }

    // Check for valid characters (alphanumeric, hyphens, underscores)
    const validNameRegex = /^[a-zA-Z0-9_-]+$/;
    if (!validNameRegex.test(name)) {
      return "Secret name can only contain letters, numbers, hyphens, and underscores";
    }

    return null;
  }

  /**
   * Validate secret value
   */
  static validateSecretValue(value: string): string | null {
    if (!value || value.trim().length === 0) {
      return "Secret value is required";
    }

    if (value.length > 10000) {
      return "Secret value must be less than 10,000 characters";
    }

    return null;
  }

  /**
   * Validate secret description
   */
  static validateSecretDescription(description?: string): string | null {
    if (description && description.length > 1000) {
      return "Secret description must be less than 1,000 characters";
    }

    return null;
  }
}
