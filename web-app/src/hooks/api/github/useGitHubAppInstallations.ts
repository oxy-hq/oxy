import { useQuery } from "@tanstack/react-query";
import { GitHubApiService } from "@/services/api";

export interface AppInstallation {
  id: number;
  name: string;
  owner_type: string;
}

export const useGitHubAppInstallations = (enabled = true) => {
  return useQuery<AppInstallation[]>({
    queryKey: ["github", "app-installations"],
    queryFn: () => GitHubApiService.listAppInstallations(),
    staleTime: 5 * 60 * 1000, // 5 min — installations rarely change
    enabled
  });
};
