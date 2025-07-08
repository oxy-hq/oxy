import { useState } from "react";
import { useNavigate } from "react-router-dom";
import { useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";

import { GitHubRepository } from "@/types/github";
import { GitHubService } from "@/services/githubService";
import { DatabaseService } from "@/services/api/database";
import queryKeys from "@/hooks/api/queryKey";

export const useOnboardingActions = () => {
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const [isSelectingRepo, setIsSelectingRepo] = useState(false);
  const [isCompletingOnboarding, setIsCompletingOnboarding] = useState(false);

  const handleRepositorySelection = async (
    repo: GitHubRepository,
    selectRepository: (repo: GitHubRepository) => void,
    refetchOnboardingState: () => void,
  ) => {
    setIsSelectingRepo(true);

    try {
      // First, set the repository in local state
      selectRepository(repo);

      // Then, call the API to actually select the repository
      const response = await GitHubService.selectRepository(repo.id);

      if (response.success) {
        await queryClient.refetchQueries({
          queryKey: queryKeys.settings.all,
        });
        toast.success("Repository selected successfully!");

        // After selecting repository, check config validation
        refetchOnboardingState();
      } else {
        throw new Error(response.message || "Failed to select repository");
      }
    } catch (error) {
      console.error("Error selecting repository:", error);
      toast.error("Failed to select repository. Please try again.");
    } finally {
      setIsSelectingRepo(false);
    }
  };

  const handleSecretsSetup = (refetchOnboardingState: () => void) => {
    // Refetch config to check if all secrets are now set
    refetchOnboardingState();
  };

  const handleSkipSecrets = () => {
    navigate("/onboarding/complete", { replace: true });
  };

  const handleCompletionSetup = async () => {
    setIsCompletingOnboarding(true);
    try {
      // Try to sync all databases, but don't fail if it errors
      try {
        await DatabaseService.syncDatabase();
      } catch (syncError) {
        console.warn("Failed to sync databases:", syncError);
      }

      // Try to build all databases, but don't fail if it errors
      try {
        await DatabaseService.buildDatabase();
      } catch (buildError) {
        console.warn("Failed to build databases:", buildError);
      }

      await GitHubService.setOnboarded(true);
      navigate("/", { replace: true });
    } catch (error) {
      console.error("Failed to complete onboarding:", error);
      toast.error("Failed to complete onboarding");
    } finally {
      setIsCompletingOnboarding(false);
    }
  };

  return {
    isSelectingRepo,
    isCompletingOnboarding,
    handleRepositorySelection,
    handleSecretsSetup,
    handleSkipSecrets,
    handleCompletionSetup,
  };
};
