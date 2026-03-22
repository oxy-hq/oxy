export interface Secret {
  id: string;
  name: string;
  description?: string;
  created_at: string;
  updated_at: string;
  created_by: string;
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

/** A secret that is sourced from an environment variable reference in config.yml */
export interface EnvSecret {
  /** The environment variable name, e.g. "SLACK_BOT_TOKEN" */
  env_var: string;
  /** The config.yml field that references this var, e.g. "slack.bot_token_var" */
  config_field: string;
  /** Whether the env var is currently set to a non-empty value */
  is_set: boolean;
  /** Masked value of the env var if set, e.g. "sk-a****bcde" */
  masked_value?: string;
}
