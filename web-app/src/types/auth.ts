export interface GoogleAuthRequest {
  code: string;
}

export interface OktaAuthRequest {
  code: string;
}

export interface GitHubAuthRequest {
  code: string;
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
}

export interface UserInfo {
  id: string;
  email: string;
  name: string;
  picture?: string;
  role: string;
  is_admin: boolean;
}

export interface MessageResponse {
  message: string;
}

export interface AuthConfigResponse {
  auth_enabled: boolean;
  google?: {
    client_id: string;
  };
  okta?: {
    client_id: string;
    domain: string;
  };
  magic_link?: boolean;
  github?: { client_id: string };
  needs_onboarding?: boolean;
  /** True when the server was started with a single fixed workspace directory. */
  single_workspace?: boolean;
  enterprise?: boolean;
  readonly: boolean;
  local_git?: boolean;
  git_remote?: boolean;
  /** The default branch name (e.g. "main", "master"). Only set in local_git mode. */
  default_branch?: string;
  /**
   * Branches where saving auto-creates a new branch instead of writing directly.
   * Configured via `protected_branches` in config.yml; defaults to [default_branch].
   * Only set in local_git mode.
   */
  protected_branches?: string[];
  /**
   * Fork point for auto-created feature branches.  Configured via `base_branch`
   * in config.yml.  When unset, new branches fork from whatever the server has
   * checked out (usually the default branch).  Use this to serve from a
   * deployment branch while forking new work from an integration branch.
   */
  base_branch?: string;
  /** Set when needs_onboarding is true due to an unexpected error (e.g. workspace directory deleted). */
  workspace_error?: string;
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

export interface InviteRequest {
  email: string;
}
