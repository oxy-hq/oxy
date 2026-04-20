import { apiClient } from "./axios";

export interface OnboardingResult {
  workspace_type: "demo" | "new" | "github";
  workspace_id: string;
}

export interface ReadinessResponse {
  has_llm_key: boolean;
  llm_keys_present: string[];
  llm_keys_missing: string[];
}

export class OnboardingService {
  static async setupDemo(orgId: string, name?: string): Promise<OnboardingResult> {
    const response = await apiClient.post<OnboardingResult>(`/orgs/${orgId}/onboarding/demo`, {
      name
    });
    return response.data;
  }

  static async setupNew(orgId: string, name?: string): Promise<OnboardingResult> {
    const response = await apiClient.post<OnboardingResult>(`/orgs/${orgId}/onboarding/new`, {
      name
    });
    return response.data;
  }

  static async setupGitHub(
    orgId: string,
    namespaceId: string,
    repoId: number,
    branch: string,
    name?: string,
    subdir?: string
  ): Promise<OnboardingResult> {
    const response = await apiClient.post<OnboardingResult>(`/orgs/${orgId}/onboarding/github`, {
      namespace_id: namespaceId,
      repo_id: repoId,
      branch,
      name,
      subdir: subdir || undefined
    });
    return response.data;
  }

  static async getReadiness(workspaceId: string): Promise<ReadinessResponse> {
    const response = await apiClient.get<ReadinessResponse>(`/${workspaceId}/onboarding-readiness`);
    return response.data;
  }
}
