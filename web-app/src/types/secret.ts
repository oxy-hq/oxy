export interface Secret {
  id: string;
  name: string;
  description?: string;
  created_at: string;
  updated_at: string;
  created_by: string;
  created_by_email?: string;
  updated_by_email?: string;
  is_active: boolean;
}

export interface CreateSecretRequest {
  name: string;
  value: string;
  description?: string;
}

export interface BulkCreateSecretsRequest {
  secrets: CreateSecretRequest[];
}

export interface BulkCreateSecretsResponse {
  created_secrets: CreateSecretResponse[];
  failed_secrets: {
    secret: CreateSecretRequest;
    error: string;
  }[];
}

export interface CreateSecretResponse {
  id: string;
  name: string;
  description?: string;
  created_at: string;
  updated_at: string;
  created_by: string;
  is_active: boolean;
}

export interface UpdateSecretRequest {
  value?: string;
  description?: string;
}

export interface SecretListResponse {
  secrets: Secret[];
  total: number;
}

export interface SecretFormData {
  name: string;
  value: string;
  description?: string;
}

export interface SecretEditFormData {
  value?: string;
  description?: string;
}

/** Where a secret's value is currently set */
export type SecretSource = "dot_env" | "environment" | "not_set";

/** A secret environment variable known to Oxy */
export interface EnvSecret {
  /** The environment variable name, e.g. "SLACK_BOT_TOKEN" */
  env_var: string;
  /** Where Oxy references this variable (config path or built-in label).
   *  null if the variable appears only in .env and is not referenced by config. */
  referenced_by: string | null;
  /** Where the secret value is currently set */
  source: SecretSource;
  /** Whether the env var is currently set to a non-empty value */
  is_set: boolean;
  /** Masked value of the env var if set, e.g. "sk-a****bcde" */
  masked_value?: string;
  /** Full plaintext value — only present for admin users */
  full_value?: string;
}
