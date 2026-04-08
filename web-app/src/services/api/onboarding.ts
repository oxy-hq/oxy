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
  static async setupDemo(name?: string): Promise<OnboardingResult> {
    const response = await apiClient.post<OnboardingResult>("/onboarding/demo", { name });
    return response.data;
  }

  static async setupNew(name?: string): Promise<OnboardingResult> {
    const response = await apiClient.post<OnboardingResult>("/onboarding/new", { name });
    return response.data;
  }

  static async setupGitHub(
    namespaceId: string,
    repoId: number,
    branch: string,
    name?: string,
    subdir?: string
  ): Promise<OnboardingResult> {
    const response = await apiClient.post<OnboardingResult>("/onboarding/github", {
      namespace_id: namespaceId,
      repo_id: repoId,
      branch,
      name,
      subdir: subdir || undefined
    });
    return response.data;
  }

  static async setupGithubUrl(opts: {
    git_url: string;
    branch?: string;
    name?: string;
    subdir?: string;
    token?: string;
  }): Promise<OnboardingResult> {
    const response = await apiClient.post<OnboardingResult>("/onboarding/github-url", opts);
    return response.data;
  }

  static async getReadiness(workspaceId?: string): Promise<ReadinessResponse> {
    const params = workspaceId ? { workspace_id: workspaceId } : {};
    const response = await apiClient.get<ReadinessResponse>("/onboarding/readiness", { params });
    return response.data;
  }
}
