import { useState } from "react";
import { Button } from "@/components/ui/shadcn/button";
import { Label } from "@/components/ui/shadcn/label";
import {
  useGitHubNamespaces,
  useGitHubRepositoriesWithApp,
  useGitHubBranchesWithApp,
} from "@/hooks/api/github";
import { Combobox } from "@/components/ui/shadcn/combobox";
import { GitHubRepository, GitHubNamespace } from "@/types/github";
import { Loader2, Github } from "lucide-react";
import SelectedRepositoryDisplay from "./SelectedRepositoryDisplay";
import { GitNamespaceSelection } from "@/components/GitNamespaceSelection";

export interface GitHubData {
  namespace: GitHubNamespace | null;
  repository: GitHubRepository | null;
  branch: string;
}

interface GitHubImportStepProps {
  isCreating: boolean;
  initialData?: GitHubData;
  onNext: (data: GitHubData) => void;
  onBack: () => void;
}

export default function GitHubImportStep({
  isCreating,
  initialData,
  onNext,
  onBack,
}: GitHubImportStepProps) {
  const [selectedGitNamespace, setSelectedGitNamespace] =
    useState<GitHubNamespace | null>(initialData?.namespace || null);

  const [selectedRepository, setSelectedRepository] =
    useState<GitHubRepository | null>(initialData?.repository || null);

  const [selectedBranch, setSelectedBranch] = useState<string>(
    initialData?.branch || "",
  );

  const { data: gitNamespaces = [], isLoading: isLoadingNamespaces } =
    useGitHubNamespaces();

  const { data: repositories = [], isLoading: isLoadingRepos } =
    useGitHubRepositoriesWithApp(selectedGitNamespace?.id || "");

  const { data: branches = [], isLoading: isLoadingBranches } =
    useGitHubBranchesWithApp(
      selectedGitNamespace?.id || "",
      selectedRepository?.full_name || "",
    );

  const isLoading =
    isLoadingNamespaces || isLoadingRepos || isLoadingBranches || isCreating;

  const isNextDisabled =
    !selectedGitNamespace || !selectedRepository || isLoading;

  const renderRepositoriesSection = () => {
    if (!selectedGitNamespace) return null;

    if (isLoadingRepos) {
      return (
        <div className="flex items-center gap-2 p-2 border rounded">
          <Loader2 className="h-4 w-4 animate-spin" />
          <span className="text-sm text-muted-foreground">
            Loading repositories...
          </span>
        </div>
      );
    }

    if (repositories.length === 0) {
      return (
        <div className="p-2 border rounded text-sm text-muted-foreground">
          No repositories found. Make sure the GitHub App has access to
          repositories.
        </div>
      );
    }

    return (
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
          setSelectedRepository(repo || null);
          setSelectedBranch("");
        }}
        placeholder="Select a repository"
        searchPlaceholder="Search repositories..."
        disabled={isLoading}
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
                        (r: GitHubRepository) => r.id.toString() === item.value,
                      )?.description
                    }
                  </div>
                )}
              </div>
            </div>
          </div>
        )}
      />
    );
  };

  const renderBranchesSection = () => {
    if (!selectedRepository) return null;

    if (isLoadingBranches) {
      return (
        <div className="flex items-center gap-2 p-2 border rounded">
          <Loader2 className="h-4 w-4 animate-spin" />
          <span className="text-sm text-muted-foreground">
            Loading branches...
          </span>
        </div>
      );
    }

    if (branches.length === 0) {
      return (
        <div className="p-2 border rounded text-sm text-muted-foreground">
          No branches found. Will use default branch:
          {selectedRepository.default_branch}
        </div>
      );
    }

    return (
      <Combobox
        items={branches.map((branch) => ({
          value: branch.name,
          label: branch.name,
        }))}
        value={selectedBranch || selectedRepository.default_branch}
        onValueChange={(value) => setSelectedBranch(value)}
        placeholder={`Select a branch (default: ${selectedRepository.default_branch})`}
        searchPlaceholder="Search branches..."
        disabled={isLoading}
      />
    );
  };

  return (
    <div className="space-y-6">
      <div>
        <h2 className="text-2xl font-bold mb-2">Import from GitHub</h2>
        <p className="text-muted-foreground">
          Connect to your GitHub repository to create a workspace
        </p>
      </div>

      <div className="space-y-4">
        <div className="space-y-6">
          <GitNamespaceSelection
            value={selectedGitNamespace?.id || ""}
            onChange={(value) => {
              const namespace = gitNamespaces.find((ns) => ns.id === value);
              setSelectedGitNamespace(namespace || null);

              setSelectedRepository(null);
              setSelectedBranch("");
            }}
          />

          {selectedGitNamespace && (
            <div className="space-y-2">
              <Label htmlFor="repository">Repository</Label>
              {renderRepositoriesSection()}
            </div>
          )}

          {selectedRepository && (
            <SelectedRepositoryDisplay repository={selectedRepository} />
          )}

          {selectedRepository && (
            <div className="space-y-2">
              <Label htmlFor="branch">Branch</Label>
              {renderBranchesSection()}
            </div>
          )}
        </div>

        <div className="flex justify-between pt-6">
          <Button
            type="button"
            variant="outline"
            onClick={onBack}
            disabled={isLoading}
          >
            Back
          </Button>
          <Button
            type="button"
            onClick={() =>
              onNext({
                namespace: selectedGitNamespace,
                repository: selectedRepository,
                branch:
                  selectedBranch || selectedRepository?.default_branch || "",
              })
            }
            disabled={isNextDisabled}
          >
            {isLoading && <Loader2 className="mr-2 h-4 w-4 animate-spin" />}
            Create Workspace
          </Button>
        </div>
      </div>
    </div>
  );
}
