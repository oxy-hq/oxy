import { useMemo } from "react";
import {
  RequiredScopesCard,
  TokenInputCard,
  RepositorySelectionCard,
  SecurityNote,
  SecretsSetup,
} from "../components";
import { SyncingStep } from "../components/SyncingStep";
import { CompletionStep } from "../components/CompletionStep";
import { GITHUB_SCOPES } from "../constants/github";
import { GitHubRepository } from "@/types/github";

interface UseStepContentsProps {
  // GitHub setup state
  token: string;
  setToken: (token: string) => void;
  isValidating: boolean;
  isValid: boolean | null;
  validateToken: () => void;
  openTokenCreationPage: () => void;
  selectedRepository: GitHubRepository | null;
  selectRepository: (repo: GitHubRepository) => void;

  // Onboarding state
  repositorySyncStatus: "idle" | "syncing" | "synced" | "error" | null;
  requiredSecrets: string[] | null;

  // Actions
  onRepositorySelect: (repo: GitHubRepository) => void;
  onSecretsSetup: () => void;
  onSkipSecrets: () => void;
  onComplete: () => void;

  // Loading states
  isSelectingRepo: boolean;
  isCompletingOnboarding: boolean;

  // Derived state
  showSecretsSetup: boolean;
}

export const useStepContents = ({
  token,
  setToken,
  isValidating,
  isValid,
  validateToken,
  openTokenCreationPage,
  selectedRepository,
  selectRepository,
  repositorySyncStatus,
  requiredSecrets,
  onRepositorySelect,
  onSecretsSetup,
  onSkipSecrets,
  onComplete,
  isSelectingRepo,
  isCompletingOnboarding,
  showSecretsSetup,
}: UseStepContentsProps) => {
  const stepContents = useMemo(
    () => ({
      token: (
        <div className="space-y-4">
          <RequiredScopesCard
            scopes={GITHUB_SCOPES}
            onOpenTokenPage={openTokenCreationPage}
          />
          <TokenInputCard
            token={token}
            onTokenChange={setToken}
            onValidate={validateToken}
            isValidating={isValidating}
            validationStatus={isValid}
          />
          {!showSecretsSetup && <SecurityNote />}
        </div>
      ),
      repository: (
        <RepositorySelectionCard
          selectedRepository={selectedRepository}
          onRepositorySelect={selectRepository}
          onProceed={() => {
            if (selectedRepository) {
              onRepositorySelect(selectedRepository);
            }
          }}
          isSelecting={isSelectingRepo}
        />
      ),
      syncing: repositorySyncStatus ? (
        <SyncingStep repositorySyncStatus={repositorySyncStatus} />
      ) : null,
      secrets: requiredSecrets ? (
        <SecretsSetup
          missingSecrets={requiredSecrets}
          onSecretsSetup={onSecretsSetup}
          onSkip={onSkipSecrets}
        />
      ) : null,
      complete: (
        <CompletionStep
          isCompletingOnboarding={isCompletingOnboarding}
          onComplete={onComplete}
        />
      ),
    }),
    [
      token,
      setToken,
      isValidating,
      isValid,
      validateToken,
      openTokenCreationPage,
      selectedRepository,
      selectRepository,
      repositorySyncStatus,
      requiredSecrets,
      onRepositorySelect,
      onSecretsSetup,
      onSkipSecrets,
      onComplete,
      isSelectingRepo,
      isCompletingOnboarding,
      showSecretsSetup,
    ],
  );

  return stepContents;
};
