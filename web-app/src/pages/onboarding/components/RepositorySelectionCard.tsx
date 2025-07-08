import { useState, useEffect } from "react";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
} from "@/components/ui/shadcn/card";
import { Combobox } from "@/components/ui/shadcn/combobox";
import { Button } from "@/components/ui/shadcn/button";
import { GitHubRepository } from "@/types/github";
import { GitHubService } from "@/services/githubService";
import { GitBranch } from "lucide-react";
import { toast } from "sonner";

interface RepositorySelectionCardProps {
  selectedRepository: GitHubRepository | null;
  onRepositorySelect: (repo: GitHubRepository) => void;
  onProceed: () => void;
  isSelecting?: boolean;
}

/**
 * Card component for GitHub repository selection
 *
 * Features:
 * - Dropdown selection of repositories
 * - Repository details display
 * - Proceed button when selection is made
 */
export const RepositorySelectionCard = ({
  selectedRepository,
  onRepositorySelect,
  onProceed,
  isSelecting = false,
}: RepositorySelectionCardProps) => {
  const [repositories, setRepositories] = useState<GitHubRepository[]>([]);
  const [isLoading, setIsLoading] = useState(false);

  // Load repositories on component mount
  useEffect(() => {
    const loadRepositories = async () => {
      try {
        setIsLoading(true);
        const repos = await GitHubService.listRepositories();
        setRepositories(repos);
      } catch (error) {
        console.error("Failed to load repositories:", error);
        toast.error("Failed to load repositories. Please try again.");
      } finally {
        setIsLoading(false);
      }
    };

    loadRepositories();
  }, []);

  const handleRepositoryChange = (value: string) => {
    const repo = repositories.find((r) => r.id.toString() === value);
    if (repo) {
      onRepositorySelect(repo);
    }
  };

  const getPlaceholderText = () => {
    if (isLoading) return "Loading repositories...";
    if (repositories.length === 0) return "No repositories found";
    return "Select a repository";
  };

  // Transform repositories into combobox items
  const comboboxItems = repositories.map((repo) => ({
    value: repo.id.toString(),
    label: repo.full_name,
    searchText:
      `${repo.full_name} ${repo.name} ${repo.description || ""}`.toLowerCase(),
  }));

  return (
    <Card>
      <CardHeader>
        <h2 className="flex items-center gap-2 text-2xl font-semibold leading-none tracking-tight">
          <GitBranch className="h-5 w-5" />
          Select Repository
        </h2>
        <CardDescription>
          Choose the GitHub repository you'd like to work with
        </CardDescription>
      </CardHeader>
      <CardContent>
        <div className="space-y-4">
          <div className="space-y-2">
            <Combobox
              items={comboboxItems}
              value={selectedRepository?.id.toString() || ""}
              onValueChange={handleRepositoryChange}
              placeholder={getPlaceholderText()}
              searchPlaceholder="Search repositories..."
              disabled={isLoading || repositories.length === 0}
              renderItem={(item) => (
                <div className="flex items-center justify-between w-full">
                  <div className="flex-1">
                    <div className="text-sm text-gray-500 dark:text-gray-400">
                      {item.label}
                    </div>
                  </div>
                </div>
              )}
            />
          </div>

          <Button
            onClick={onProceed}
            disabled={!selectedRepository || isSelecting}
            className="w-full"
          >
            {isSelecting ? "Setting up repository..." : <>Select</>}
          </Button>
        </div>
      </CardContent>
    </Card>
  );
};
