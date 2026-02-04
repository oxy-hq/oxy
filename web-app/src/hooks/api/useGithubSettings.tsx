import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { GitHubService } from "@/services/githubService";
import type { GitHubSettings, RevisionInfo } from "@/types/settings";
import queryKeys from "./queryKey";

const useGithubSettings = (
  enabled = true,
  refetchOnWindowFocus = true,
  refetchOnMount: boolean | "always" = false
) =>
  useQuery<GitHubSettings, Error>({
    queryKey: queryKeys.settings.all,
    queryFn: () => GitHubService.getGithubSettings(),
    enabled,
    refetchOnWindowFocus,
    refetchOnMount
  });

const useRevisionInfo = (
  enabled = true,
  refetchOnWindowFocus = false,
  refetchOnMount: boolean | "always" = false
) =>
  useQuery<RevisionInfo, Error>({
    queryKey: queryKeys.settings.revisionInfo(),
    queryFn: () => GitHubService.getGithubRevisionInfo(),
    enabled,
    refetchOnWindowFocus,
    refetchOnMount,
    retry: 2,
    staleTime: 30000 // Consider data stale after 30 seconds
  });

const useUpdateGitHubToken = () => {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (token: string) => GitHubService.updateGitHubToken(token),
    onSuccess: () => {
      toast.success("GitHub token updated successfully");
      queryClient.invalidateQueries({ queryKey: queryKeys.settings.all });
      queryClient.invalidateQueries({
        queryKey: queryKeys.settings.revisionInfo()
      });
    },
    onError: (error) => {
      console.error("Error updating GitHub token:", error);
      toast.error("Failed to update GitHub token");
    }
  });
};

const useSelectRepository = () => {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (repositoryId: number) => GitHubService.selectRepository(repositoryId),
    onSuccess: () => {
      toast.success("Repository selected successfully");
      queryClient.invalidateQueries({ queryKey: queryKeys.settings.all });
      queryClient.invalidateQueries({
        queryKey: queryKeys.settings.revisionInfo()
      });
    },
    onError: (error) => {
      console.error("Error selecting repository:", error);
      toast.error("Failed to select repository");
    }
  });
};

const useListRepositories = () => {
  return useQuery({
    queryKey: queryKeys.repositories.all,
    queryFn: GitHubService.listRepositories,
    enabled: false, // Only fetch when explicitly triggered
    staleTime: 5 * 60 * 1000 // 5 minutes
  });
};

const useSyncGitHubRepository = () => {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: () => GitHubService.syncGitHubRepository(),
    onSuccess: (data) => {
      toast.success(data.message || "Repository synced successfully");
      queryClient.invalidateQueries({ queryKey: queryKeys.settings.all });
      queryClient.invalidateQueries({
        queryKey: queryKeys.settings.revisionInfo()
      });
    },
    onError: (error) => {
      console.error("Error syncing repository:", error);
      toast.error("Failed to sync repository");
    }
  });
};

export {
  useGithubSettings,
  useRevisionInfo,
  useUpdateGitHubToken,
  useSelectRepository,
  useListRepositories,
  useSyncGitHubRepository
};
