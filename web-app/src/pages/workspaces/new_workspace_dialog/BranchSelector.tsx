import React from "react";
import { Label } from "@/components/ui/shadcn/label";
import { Combobox } from "@/components/ui/shadcn/combobox";
import { Loader2 } from "lucide-react";
import { GitHubRepository } from "@/types/github";
import { useGitHubBranches } from "@/hooks/api/github/useGitHubBranches";

interface GitHubBranch {
  name: string;
}

interface BranchSelectorProps {
  token: string;
  selectedBranch: string;
  onBranchChange: (branch: string) => void;
  repository: GitHubRepository;
  disabled?: boolean;
}

export const BranchSelector: React.FC<BranchSelectorProps> = ({
  token,
  selectedBranch,
  onBranchChange,
  repository,
  disabled = false,
}) => {
  const {
    data: branches = [],
    isLoading,
    error: branchesError,
  } = useGitHubBranches(token, repository.full_name);

  return (
    <div className="space-y-2">
      <Label htmlFor="branch">Active Branch</Label>
      {isLoading && (
        <div className="flex items-center gap-2 p-2 border rounded">
          <Loader2 className="h-4 w-4 animate-spin" />
          <span className="text-sm text-muted-foreground">
            Loading branches...
          </span>
        </div>
      )}
      {!isLoading && branches.length > 0 && (
        <Combobox
          items={branches.map((branch: GitHubBranch) => ({
            value: branch.name,
            label: branch.name,
            searchText: branch.name.toLowerCase(),
          }))}
          value={selectedBranch}
          onValueChange={onBranchChange}
          placeholder={`Select a active branch (default: ${repository.default_branch})`}
          searchPlaceholder="Search branches..."
          disabled={disabled}
          renderItem={(item) => (
            <div className="flex items-center justify-between w-full">
              <div className="flex items-center gap-2">
                <div className="text-sm font-medium">
                  {item.label}
                  {item.value === repository.default_branch && (
                    <span className="ml-2 text-xs bg-muted px-1.5 py-0.5 rounded">
                      default
                    </span>
                  )}
                </div>
              </div>
            </div>
          )}
        />
      )}
      {!isLoading && branches.length === 0 && repository && (
        <div className="p-2 border rounded text-sm text-muted-foreground">
          No branches found. The default branch will be used.
        </div>
      )}
      {branchesError && (
        <p className="text-sm text-red-600">
          Failed to load branches. The default branch will be used.
        </p>
      )}
    </div>
  );
};
