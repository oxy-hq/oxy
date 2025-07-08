export interface ConfigStatusResponse {
  config_valid: boolean;
  required_secrets: string[] | null;
}

export interface SecretInputFormData {
  [secretName: string]: string;
}
