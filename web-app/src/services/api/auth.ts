import type {
  AuthConfigResponse,
  AuthResponse,
  GitHubAuthRequest,
  GoogleAuthRequest,
  MagicLinkRequest,
  MagicLinkVerifyRequest,
  MessageResponse,
  OAuthStateResponse,
  OktaAuthRequest
} from "@/types/auth";
import { apiClient } from "./axios";

export class AuthService {
  /**
   * Mint a short-lived signed OAuth state token. Call before redirecting the
   * user to an external provider; echo the returned `state` through the
   * provider's `state` query param and back to the `/auth/{provider}` endpoint.
   */
  static async issueOAuthState(): Promise<OAuthStateResponse> {
    const response = await apiClient.post("/auth/oauth/state");
    return response.data;
  }

  static async googleAuth(request: GoogleAuthRequest): Promise<AuthResponse> {
    const response = await apiClient.post("/auth/google", request);
    return response.data;
  }

  static async oktaAuth(request: OktaAuthRequest): Promise<AuthResponse> {
    const response = await apiClient.post("/auth/okta", request);
    return response.data;
  }

  static async githubAuth(request: GitHubAuthRequest): Promise<AuthResponse> {
    const response = await apiClient.post("/auth/github", request);
    return response.data;
  }

  static async getAuthConfig(): Promise<AuthConfigResponse> {
    const response = await apiClient.get("/auth/config");
    return response.data;
  }

  static async requestMagicLink(request: MagicLinkRequest): Promise<MessageResponse> {
    const response = await apiClient.post("/auth/magic-link/request", request);
    return response.data;
  }

  static async verifyMagicLink(request: MagicLinkVerifyRequest): Promise<AuthResponse> {
    const response = await apiClient.post("/auth/magic-link/verify", request);
    return response.data;
  }
}
