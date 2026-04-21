export interface GoogleAuthRequest {
  code: string;
  /** Opaque JWT issued by `POST /auth/oauth/state`. Required; backend rejects with 422 if absent and 401 if invalid. */
  state: string;
}

export interface OktaAuthRequest {
  code: string;
  /** See `GoogleAuthRequest.state`. */
  state: string;
}

export interface GitHubAuthRequest {
  code: string;
  /** See `GoogleAuthRequest.state`. */
  state: string;
}

export interface OAuthStateResponse {
  state: string;
}

export interface MagicLinkRequest {
  email: string;
}

export interface MagicLinkVerifyRequest {
  token: string;
}

export interface AuthResponse {
  token: string;
  user: UserInfo;
  orgs: OrgInfo[];
}

export interface OrgInfo {
  id: string;
  name: string;
  slug: string;
  role: string;
}

/**
 * Global profile fields. Role / admin status are per-org; read from the
 * `orgs` array in the login response or via `GET /orgs`.
 */
export interface UserInfo {
  id: string;
  email: string;
  name: string;
  picture?: string;
  status?: string;
}

export interface MessageResponse {
  message: string;
}

export type ServeMode = "local" | "cloud";

export interface AuthConfigResponse {
  auth_enabled: boolean;
  /**
   * Deployment mode set by the backend. In `local` mode the server skips auth,
   * exposes a reduced route surface, and uses the nil-UUID workspace — the
   * frontend must mirror that by hiding org/auth/workspace-management UI.
   */
  mode: ServeMode;
  google?: {
    client_id: string;
  };
  okta?: {
    client_id: string;
    domain: string;
  };
  magic_link?: boolean;
  github?: { client_id: string };
  enterprise?: boolean;
  /**
   * True when the observability backend is wired up (OXY_OBSERVABILITY_BACKEND
   * set to duckdb/postgres/clickhouse). When false on an enterprise build,
   * observability pages render a "not configured" banner and record nothing.
   * Always present — server serializes the bool unconditionally.
   */
  observability_enabled: boolean;
}
