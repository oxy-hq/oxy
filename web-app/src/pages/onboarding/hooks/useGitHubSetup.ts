import { useState } from "react";
import { useNavigate } from "react-router-dom";
import { useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { GitHubService } from "@/services/githubService";
import { GitHubRepository } from "@/types/github";
import { GITHUB_TOKEN_URL } from "../constants/github";
import { openSecureWindow } from "../utils/github";
import queryKeys from "@/hooks/api/queryKey";
import { useProjectStatus } from "@/hooks/useProjectStatus";

interface UseGitHubSetupState {
  // Token validation state
  token: string;
  isValidating: boolean;
  isValid: boolean | null;

  // Repository selection state
  selectedRepository: GitHubRepository | null;
  isSelectingRepository: boolean;
}

interface UseGitHubSetupActions {
  setToken: (token: string) => void;
  validateToken: () => Promise<void>;
  openTokenCreationPage: () => void;
  selectRepository: (repo: GitHubRepository) => void;
  proceedWithRepository: () => Promise<void>;
}

/**
 * Combined hook for GitHub token validation and repository selection
 *
 * This hook manages the complete GitHub setup workflow:
 * 1. Token input and validation
 * 2. Repository selection and final setup
 */
export const useGitHubSetup = (): UseGitHubSetupState &
  UseGitHubSetupActions => {
  const navigate = useNavigate();
  const queryClient = useQueryClient();

  // Token validation state
  const [token, setToken] = useState("");
  const [isValidating, setIsValidating] = useState(false);
  const [isValid, setIsValid] = useState<boolean | null>(null);

  // Repository selection state
  const [selectedRepository, setSelectedRepository] =
    useState<GitHubRepository | null>(null);
  const [isSelectingRepository, setIsSelectingRepository] = useState(false);
  const { data: projectStatus } = useProjectStatus();

  const validateToken = async () => {
    if (!token.trim()) {
      toast.error("Please enter a GitHub token");
      return;
    }

    setIsValidating(true);
    setIsValid(null);

    try {
      await GitHubService.storeToken(token);
      setIsValid(true);
      toast.success("GitHub token validated successfully!");
    } catch (error) {
      setIsValid(false);
      toast.error(
        "Invalid GitHub token. Please check your token and try again.",
      );
      console.error("Token validation error:", error);
    } finally {
      setIsValidating(false);
      // invalidate project status to refresh onboarding state
      await queryClient.invalidateQueries({
        queryKey: queryKeys.settings.projectStatus(),
      });
    }
  };

  const selectRepository = (repo: GitHubRepository) => {
    setSelectedRepository(repo);
  };

  const proceedWithRepository = async () => {
    if (!selectedRepository) {
      toast.error("Please select a repository");
      return;
    }

    setIsSelectingRepository(true);

    try {
      const response = await GitHubService.selectRepository(
        selectedRepository.id,
      );

      if (response.success) {
        await queryClient.refetchQueries({
          queryKey: queryKeys.settings.all,
        });
        toast.success("Repository selected successfully!");

        // Check for required secrets before proceeding
        try {
          if (projectStatus?.is_config_valid) {
            if (projectStatus.required_secrets) {
              // Navigate to secrets setup if there are missing secrets
              navigate("/onboarding/secrets");
            } else {
              // Navigate to complete if all secrets are available
              navigate("/onboarding/complete");
            }
          } else {
            // show error and allow reselecting repository
            toast.error(
              "Selected repository does not have valid configuration. Please select a different repository.",
            );
            setSelectedRepository(null);
          }
        } catch (configError) {
          console.warn(
            "Failed to check config validation, proceeding to complete:",
            configError,
          );
          // If validation fails, proceed to complete page anyway
          navigate("/onboarding/complete");
        }
      } else {
        throw new Error(response.message || "Failed to select repository");
      }
    } catch (error) {
      console.error("Failed to select repository:", error);
      toast.error("Failed to select repository. Please try again.");
    } finally {
      setIsSelectingRepository(false);
      // Invalidate project status to refresh onboarding state
      await queryClient.invalidateQueries({
        queryKey: queryKeys.settings.projectStatus(),
      });
    }
  };

  const openTokenCreationPage = () => {
    openSecureWindow(GITHUB_TOKEN_URL);
  };

  return {
    // Token validation state
    token,
    isValidating,
    isValid,

    // Repository selection state
    selectedRepository,
    isSelectingRepository,

    // Actions
    setToken,
    validateToken,
    openTokenCreationPage,
    selectRepository,
    proceedWithRepository,
  };
};
