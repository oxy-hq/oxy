import type {
  AuthConfigResponse,
  AuthResponse,
  GoogleAuthRequest,
  MagicLinkRequest,
  MagicLinkVerifyRequest,
  MessageResponse,
  OktaAuthRequest
} from "@/types/auth";
import { apiClient } from "./axios";

export class AuthService {
  static async googleAuth(request: GoogleAuthRequest): Promise<AuthResponse> {
    const response = await apiClient.post("/auth/google", request);
    return response.data;
  }

  static async oktaAuth(request: OktaAuthRequest): Promise<AuthResponse> {
    const response = await apiClient.post("/auth/okta", request);
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
