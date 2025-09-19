import React from "react";
import { Label } from "@/components/ui/shadcn/label";
import { Combobox } from "@/components/ui/shadcn/combobox";
import { Loader2, Github } from "lucide-react";
import { GitHubRepository } from "@/types/github";
import { useGitHubRepositories } from "@/hooks/api/github/useGitHubRepositories";

interface RepositorySelectorProps {
  token: string;
  selectedRepository: GitHubRepository | null;
  onRepositoryChange: (repository: GitHubRepository | null) => void;
  disabled?: boolean;
}

export const RepositorySelector: React.FC<RepositorySelectorProps> = ({
  token,
  selectedRepository,
  onRepositoryChange,
  disabled = false,
}) => {
  const {
    data: repositories = [],
    isLoading,
    error: repositoriesError,
  } = useGitHubRepositories(token);

  if (!token) {
    return null;
  }

  return (
    <div className="space-y-2">
      <Label htmlFor="repository">Repository</Label>
      {isLoading && (
        <div className="flex items-center gap-2 p-2 border rounded">
          <Loader2 className="h-4 w-4 animate-spin" />
          <span className="text-sm text-muted-foreground">
            Loading repositories...
          </span>
        </div>
      )}
      {!isLoading && repositories.length > 0 && (
        <Combobox
          items={repositories.map((repo: GitHubRepository) => ({
            value: repo.id.toString(),
            label: repo.full_name,
            searchText:
              `${repo.full_name} ${repo.name} ${repo.description || ""}`.toLowerCase(),
          }))}
          value={selectedRepository?.id.toString() || ""}
          onValueChange={(value) => {
            const repo = repositories.find(
              (r: GitHubRepository) => r.id.toString() === value,
            );
            onRepositoryChange(repo || null);
          }}
          placeholder="Select a repository"
          searchPlaceholder="Search repositories..."
          disabled={disabled}
          renderItem={(item) => (
            <div className="flex items-center justify-between w-full">
              <div className="flex items-center gap-2">
                <Github className="h-4 w-4 text-muted-foreground" />
                <div>
                  <div className="text-sm font-medium">{item.label}</div>
                  {repositories.find(
                    (r: GitHubRepository) => r.id.toString() === item.value,
                  )?.description && (
                    <div className="text-xs text-muted-foreground">
                      {
                        repositories.find(
                          (r: GitHubRepository) =>
                            r.id.toString() === item.value,
                        )?.description
                      }
                    </div>
                  )}
                </div>
              </div>
            </div>
          )}
        />
      )}
      {!isLoading && repositories.length === 0 && token && (
        <div className="p-2 border rounded text-sm text-muted-foreground">
          No repositories found. Make sure your token has access to
          repositories.
        </div>
      )}
      {repositoriesError && (
        <p className="text-sm text-red-600">
          Failed to load repositories. Please check your token.
        </p>
      )}
    </div>
  );
};
