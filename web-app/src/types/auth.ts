export interface LoginRequest {
  email: string;
  password: string;
}

export interface RegisterRequest {
  email: string;
  password: string;
  name: string;
}

export interface GoogleAuthRequest {
  code: string;
}

export interface ValidateEmailRequest {
  token: string;
}

export interface AuthResponse {
  token: string;
  user: UserInfo;
}

export interface UserInfo {
  id: string;
  email: string;
  name: string;
  picture?: string;
}

export interface MessageResponse {
  message: string;
}

export interface AuthConfigResponse {
  is_built_in_mode: boolean;
  auth_enabled: boolean;
  google?: {
    client_id: string;
  };
  basic?: boolean;
}
