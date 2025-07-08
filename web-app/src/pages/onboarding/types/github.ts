export interface GitHubTokenValidationState {
  token: string;
  isValidating: boolean;
  isValid: boolean | null;
}

export interface GitHubTokenValidationActions {
  setToken: (token: string) => void;
  validateToken: () => Promise<void>;
  openTokenCreationPage: () => void;
}

export type ValidationStatus = boolean | null;
