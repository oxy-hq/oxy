import { useNavigate } from "react-router-dom";
import { useEffect } from "react";

import {
  LoadingScreen,
  ErrorMessage,
  SuccessHeader,
  ProjectCard,
  ActionButton,
} from "./components";
import { useOnboardingState } from "./hooks/useOnboardingState";
import { useQueryClient } from "@tanstack/react-query";
import { useProjectStatus } from "@/hooks/useProjectStatus";

const SetupComplete = () => {
  const { data: projectStatus, isLoading, error } = useProjectStatus();
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const onboardingState = useOnboardingState();

  // Redirect if onboarding is not actually complete
  useEffect(() => {
    if (onboardingState.isLoading) return;

    if (onboardingState.step !== "complete") {
      // Onboarding is not complete, redirect to appropriate step
      if (
        onboardingState.step === "token" ||
        onboardingState.step === "repository" ||
        onboardingState.step === "syncing" ||
        onboardingState.step === "secrets"
      ) {
        navigate("/onboarding/setup", { replace: true });
      }
    }
  }, [onboardingState.step, onboardingState.isLoading, navigate]);

  const handleGoHome = () => {
    // clear all local state or cache
    sessionStorage.clear();
    localStorage.clear();

    // invalidate auth query
    queryClient.invalidateQueries({ queryKey: ["authConfig"] });

    navigate("/", { replace: true });
  };

  if (isLoading || onboardingState.isLoading) {
    return <LoadingScreen />;
  }

  // Only show setup complete if onboarding is actually complete
  if (onboardingState.step !== "complete") {
    return <LoadingScreen />;
  }

  return (
    <div className="w-full min-h-screen bg-secondary flex items-center justify-center p-4">
      <div className="max-w-2xl">
        <SuccessHeader />

        {error && (
          <ErrorMessage message={error.message || "An error occurred"} />
        )}

        {projectStatus?.repository && (
          <ProjectCard
            project={{
              repository: projectStatus.repository,
              sync_status: projectStatus.repository_sync_status,
            }}
          />
        )}

        <ActionButton onClick={handleGoHome} />
      </div>
    </div>
  );
};

export default SetupComplete;
