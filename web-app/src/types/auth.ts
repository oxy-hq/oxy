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

export interface OktaAuthRequest {
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
  role: string;
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
  okta?: {
    client_id: string;
    domain: string;
  };
  basic?: boolean;
  cloud?: boolean;
  enterprise?: boolean;
  readonly: boolean;
}

export interface UserListResponse {
  users: UserInfo[];
  total: number;
}

export interface CreateUserRequest {
  email: string;
  name: string;
  role?: string;
}

export interface UpdateUserRoleRequest {
  role: string;
}
