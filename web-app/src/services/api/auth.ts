import { apiClient } from "./axios";
import {
  LoginRequest,
  RegisterRequest,
  GoogleAuthRequest,
  OktaAuthRequest,
  ValidateEmailRequest,
  AuthResponse,
  MessageResponse,
  AuthConfigResponse,
} from "@/types/auth";

export class AuthService {
  static async login(request: LoginRequest): Promise<AuthResponse> {
    const response = await apiClient.post("/auth/login", request);
    return response.data;
  }

  static async register(request: RegisterRequest): Promise<MessageResponse> {
    const response = await apiClient.post("/auth/register", request);
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

  static async validateEmail(
    request: ValidateEmailRequest,
  ): Promise<AuthResponse> {
    const response = await apiClient.post("/auth/validate_email", request);
    return response.data;
  }

  static async getAuthConfig(): Promise<AuthConfigResponse> {
    const response = await apiClient.get("/auth/config");
    return response.data;
  }
}
