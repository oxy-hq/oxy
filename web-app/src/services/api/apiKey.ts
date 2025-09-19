import { apiClient } from "./axios";
import {
  ApiKey,
  ApiKeyListResponse,
  CreateApiKeyRequest,
  CreateApiKeyResponse,
} from "@/types/apiKey";

export class ApiKeyService {
  static async createApiKey(
    projectId: string,
    request: CreateApiKeyRequest,
  ): Promise<CreateApiKeyResponse> {
    const response = await apiClient.post<CreateApiKeyResponse>(
      `/${projectId}/api-keys`,
      request,
    );
    return response.data;
  }

  static async listApiKeys(projectId: string): Promise<ApiKeyListResponse> {
    const response = await apiClient.get<ApiKeyListResponse>(
      `/${projectId}/api-keys`,
    );
    return response.data;
  }

  static async getApiKey(projectId: string, id: string): Promise<ApiKey> {
    const response = await apiClient.get<ApiKey>(
      `/${projectId}/api-keys/${id}`,
    );
    return response.data;
  }

  static async revokeApiKey(projectId: string, id: string): Promise<void> {
    await apiClient.delete(`/${projectId}/api-keys/${id}`);
  }

  static maskApiKey(key: string): string {
    if (key.length <= 8) return key;
    return `${key.slice(0, 8)}${"*".repeat(Math.max(0, key.length - 12))}${key.slice(-4)}`;
  }

  static formatDate(dateString: string): string {
    return new Date(dateString).toLocaleDateString("en-US", {
      year: "numeric",
      month: "short",
      day: "numeric",
      hour: "2-digit",
      minute: "2-digit",
    });
  }

  static isExpired(expiresAt?: string): boolean {
    if (!expiresAt) return false;
    return new Date(expiresAt) < new Date();
  }

  static getTimeUntilExpiration(expiresAt?: string): string | null {
    if (!expiresAt) return null;

    const expirationDate = new Date(expiresAt);
    const now = new Date();
    const diffMs = expirationDate.getTime() - now.getTime();

    if (diffMs <= 0) return "Expired";

    const days = Math.floor(diffMs / (1000 * 60 * 60 * 24));
    if (days > 0) return `${days} day${days === 1 ? "" : "s"}`;

    const hours = Math.floor(diffMs / (1000 * 60 * 60));
    if (hours > 0) return `${hours} hour${hours === 1 ? "" : "s"}`;

    const minutes = Math.floor(diffMs / (1000 * 60));
    return `${minutes} minute${minutes === 1 ? "" : "s"}`;
  }
}
