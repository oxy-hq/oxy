import { useState, useEffect, useMemo } from "react";
import { Navigate } from "react-router-dom";
import { useInterval } from "usehooks-ts";

import { useGitHubSetup } from "./hooks/useGitHubSetup";
import { useOnboardingState } from "./hooks/useOnboardingState";
import { useOnboardingActions } from "./hooks/useOnboardingActions";
import { useStepContents } from "./hooks/useStepContents";
import { LoadingScreen, PageHeader, OnboardingSteps } from "./components";

const SetupPage = () => {
  const [isRepositorySetupComplete, setIsRepositorySetupComplete] =
    useState(false);
  const onboardingState = useOnboardingState();
  const githubSetup = useGitHubSetup();
  const {
    isSelectingRepo,
    isCompletingOnboarding,
    handleRepositorySelection,
    handleSecretsSetup,
    handleSkipSecrets,
    handleCompletionSetup,
  } = useOnboardingActions();

  // Poll for syncing status updates
  useInterval(() => {
    if (onboardingState.repositorySyncStatus === "syncing") {
      onboardingState.refetch();
    }
  }, 1000);

  // Handle automatic state updates based on onboarding step
  useEffect(() => {
    if (onboardingState.isLoading) return;

    const shouldShowRepositorySetup = [
      "complete",
      "secrets",
      "syncing",
      "repository",
    ].includes(onboardingState.step || "");

    setIsRepositorySetupComplete(shouldShowRepositorySetup);
  }, [onboardingState.step, onboardingState.isLoading]);

  // Memoize derived state
  const showSecretsSetup = useMemo(() => {
    return (
      isRepositorySetupComplete &&
      onboardingState.repositorySyncStatus === "synced" &&
      onboardingState?.requiredSecrets &&
      onboardingState.requiredSecrets.length > 0
    );
  }, [
    isRepositorySetupComplete,
    onboardingState.repositorySyncStatus,
    onboardingState.requiredSecrets,
  ]);

  // Prepare step contents
  const stepContents = useStepContents({
    // GitHub setup state
    token: githubSetup.token,
    setToken: githubSetup.setToken,
    isValidating: githubSetup.isValidating,
    isValid: githubSetup.isValid,
    validateToken: githubSetup.validateToken,
    openTokenCreationPage: githubSetup.openTokenCreationPage,
    selectedRepository: githubSetup.selectedRepository,
    selectRepository: githubSetup.selectRepository,

    // Onboarding state
    repositorySyncStatus: onboardingState.repositorySyncStatus,
    requiredSecrets: onboardingState.requiredSecrets,

    // Actions
    onRepositorySelect: (repo) =>
      handleRepositorySelection(
        repo,
        githubSetup.selectRepository,
        onboardingState.refetch,
      ),
    onSecretsSetup: () => handleSecretsSetup(onboardingState.refetch),
    onSkipSecrets: handleSkipSecrets,
    onComplete: handleCompletionSetup,

    // Loading states
    isSelectingRepo,
    isCompletingOnboarding,

    // Derived state
    showSecretsSetup: showSecretsSetup ?? false,
  });

  // Early returns for different states
  if (onboardingState.isLoading) {
    return <LoadingScreen />;
  }

  if (onboardingState.isOnboarded) {
    return <Navigate to="/" replace />;
  }

  return (
    <div className="min-h-screen w-full bg-secondary flex p-4 overflow-y-auto">
      <div className="max-w-4xl mx-auto w-full">
        <PageHeader title="Project Setup" />
        <OnboardingSteps
          currentStep={onboardingState.step}
          stepContents={stepContents}
        />
      </div>
    </div>
  );
};

export default SetupPage;
