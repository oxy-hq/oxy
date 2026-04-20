import { useQuery } from "@tanstack/react-query";
import queryKeys from "@/hooks/api/queryKey";
import { GitHubApiService } from "@/services/api";
import type { GitHubAccount } from "@/types/github";

export const useGitHubAccount = () =>
  useQuery<GitHubAccount, Error>({
    queryKey: queryKeys.github.account,
    queryFn: () => GitHubApiService.getAccount()
  });
