import { apiClient } from "./axios";
import {
  ApiKey,
  ApiKeyListResponse,
  CreateApiKeyRequest,
  CreateApiKeyResponse,
} from "@/types/apiKey";
const BASE_PATH = "/api-keys";

export class ApiKeyService {
  /**
   * Create a new API key
   */
  static async createApiKey(
    request: CreateApiKeyRequest,
  ): Promise<CreateApiKeyResponse> {
    const response = await apiClient.post<CreateApiKeyResponse>(
      BASE_PATH,
      request,
    );
    return response.data;
  }

  /**
   * List all API keys for the current user
   */
  static async listApiKeys(): Promise<ApiKeyListResponse> {
    const response = await apiClient.get<ApiKeyListResponse>(BASE_PATH);
    return response.data;
  }

  /**
   * Get details of a specific API key
   */
  static async getApiKey(id: string): Promise<ApiKey> {
    const response = await apiClient.get<ApiKey>(`${BASE_PATH}/${id}`);
    return response.data;
  }

  /**
   * Revoke (delete) an API key
   */
  static async revokeApiKey(id: string): Promise<void> {
    await apiClient.delete(`${BASE_PATH}/${id}`);
  }

  /**
   * Mask an API key for safe display
   */
  static maskApiKey(key: string): string {
    if (key.length <= 8) return key;
    return `${key.slice(0, 8)}${"*".repeat(Math.max(0, key.length - 12))}${key.slice(-4)}`;
  }

  /**
   * Format date for display
   */
  static formatDate(dateString: string): string {
    return new Date(dateString).toLocaleDateString("en-US", {
      year: "numeric",
      month: "short",
      day: "numeric",
      hour: "2-digit",
      minute: "2-digit",
    });
  }

  /**
   * Check if an API key is expired
   */
  static isExpired(expiresAt?: string): boolean {
    if (!expiresAt) return false;
    return new Date(expiresAt) < new Date();
  }

  /**
   * Get time until expiration
   */
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
