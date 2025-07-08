import { useMemo } from "react";
import { useProjectStatus } from "@/hooks/useProjectStatus";
import { GitHubRepository, RepositorySyncStatus } from "@/types/github";

export type OnboardingStep =
  | "token"
  | "repository"
  | "syncing"
  | "secrets"
  | "complete";

export interface OnboardingState {
  isLoading: boolean;
  currentStep: OnboardingStep;
  step: OnboardingStep; // Alternative property name for backwards compatibility
  hasRepository: boolean;
  repositorySyncStatus: RepositorySyncStatus | null;
  repository?: GitHubRepository;
  refetch: () => void;
  isOnboarded: boolean;
  requiredSecrets: string[] | null;
}

/**
 * Hook to determine the current onboarding state based on project status
 *
 * This hook analyzes the project status from useProjectStatus and determines
 * which step of the onboarding process the user is currently on.
 *
 * Steps:
 * - token: User needs to provide GitHub token and select repository
 * - repository: Repository selected but not synced yet
 * - syncing: Repository is currently being cloned/synced
 * - secrets: Repository synced but required secrets are missing
 * - complete: All onboarding steps are completed
 */
export const useOnboardingState = (): OnboardingState => {
  const { data: projectStatus, isLoading, error, refetch } = useProjectStatus();

  const onboardingState = useMemo(() => {
    // If still loading or there's an error, assume we need GitHub connection
    if (isLoading || error || !projectStatus) {
      return {
        isLoading: isLoading,
        currentStep: "token" as OnboardingStep,
        step: "token" as OnboardingStep,
        hasRepository: false,
        repositorySyncStatus: null,
        repository: undefined,
        isOnboarded: false,
        requiredSecrets: null,
        refetch,
      };
    }

    const { github_connected, repository, required_secrets, is_onboarded } =
      projectStatus;

    // Determine if repository is synced (exists and config is valid)
    const hasRepository = !!repository;
    const isRepositorySynced =
      hasRepository && projectStatus.repository_sync_status === "synced";
    const hasRequiredSecrets =
      !!required_secrets && required_secrets.length > 0;

    let currentStep: OnboardingStep;
    console.log(
      "useOnboardingState projectStatus",
      projectStatus,
      "hasRepository",
      hasRepository,
      "isRepositorySynced",
      isRepositorySynced,
      "hasRequiredSecrets",
      hasRequiredSecrets,
      "github_connected",
      github_connected,
      "is_onboarded",
      is_onboarded,
    );

    if (hasRepository && isRepositorySynced && !hasRequiredSecrets) {
      // All steps complete
      currentStep = "complete";
    } else if (hasRepository && isRepositorySynced && hasRequiredSecrets) {
      // Repository is synced but secrets are missing
      currentStep = "secrets";
    } else if (hasRepository && !isRepositorySynced) {
      // Repository exists but not synced (could be syncing or needs config)
      currentStep = "syncing";
    } else if (github_connected && !hasRepository) {
      // GitHub token is connected but no repository selected
      currentStep = "repository";
    } else {
      // No GitHub connection or starting fresh
      currentStep = "token";
    }
    console.log("currentStep", currentStep);

    return {
      isLoading: false,
      currentStep,
      step: currentStep, // Backwards compatibility
      hasRepository,
      repositorySyncStatus: projectStatus.repository_sync_status,
      hasRequiredSecrets,
      repository,
      refetch,
      isOnboarded: is_onboarded,
      requiredSecrets: projectStatus.required_secrets ?? null,
    };
  }, [projectStatus, isLoading, error, refetch]);

  return onboardingState;
};
