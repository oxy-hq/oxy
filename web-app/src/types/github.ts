import type { AuthResponse } from "@/types/auth";

export interface GitHubRepository {
  id: number;
  name: string;
  full_name: string;
  html_url: string;
  description?: string;
  default_branch: string;
  updated_at: string;
  clone_url: string;
}

export interface GitHubBranch {
  name: string;
}

export interface GitHubNamespace {
  id: string;
  owner_type: string;
  slug: string;
  name: string;
}

export interface ProjectStatus {
  required_secrets?: string[];
  is_config_valid: boolean;
  error?: string;
}

export type GitHubAccount = {
  connected: boolean;
  github_login?: string;
};

export type UserInstallation = {
  id: number;
  account_login: string;
  account_type: string;
};

export interface GitHubCallbackBody {
  state: string;
  code?: string;
  installation_id?: number;
}

export type GitHubCallbackResponse =
  | { flow: "oauth"; login: string }
  | { flow: "install"; namespace_id: string }
  | { flow: "auth"; auth: AuthResponse };
