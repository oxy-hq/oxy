import { apiClient } from "./axios";

export interface MobileAuthorizeRequest {
  code_challenge: string;
  code_challenge_method: "S256";
  redirect_uri: string;
}

export interface MobileAuthorizeResponse {
  code: string;
}

/**
 * The browser half of the Oxygen mobile app's sign-in (PKCE): trade the session for a single-use
 * code bound to the app's challenge and `oxygen://` redirect URI. Session-only on the server.
 */
export const MobileAuthService = {
  async authorize(request: MobileAuthorizeRequest): Promise<MobileAuthorizeResponse> {
    const response = await apiClient.post<MobileAuthorizeResponse>(
      "/auth/mobile/authorize",
      request
    );
    return response.data;
  }
};
