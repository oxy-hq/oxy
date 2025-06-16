export interface ApiKey {
  id: string;
  name: string;
  expires_at?: string;
  last_used_at?: string;
  created_at: string;
  is_active: boolean;
  masked_key?: string; // Only shown for newly created keys
}

export interface CreateApiKeyRequest {
  name: string;
  expires_at?: string;
}

export interface CreateApiKeyResponse {
  id: string;
  key: string; // Full key - only returned on creation
  name: string;
  expires_at?: string;
  created_at: string;
  masked_key: string;
}

export interface ApiKeyListResponse {
  api_keys: ApiKey[];
  total: number;
}

export interface ApiKeyFormData {
  name: string;
  expiresAt?: Date;
}
