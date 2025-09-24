import React, { useState } from "react";
import { useCreateWorkspace } from "@/hooks/api/workspaces/useWorkspaces";
import { Button } from "@/components/ui/shadcn/button";
import { Input } from "@/components/ui/shadcn/input";
import { Label } from "@/components/ui/shadcn/label";

import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/shadcn/dialog";
import { Plus, Loader2 } from "lucide-react";
import { toast } from "sonner";
import { GitHubTokenInput } from "./GitHubTokenInput";
import { RepositorySelector } from "./RepositorySelector";
import { SelectedRepositoryDisplay } from "./SelectedRepositoryDisplay";
import { BranchSelector } from "./BranchSelector";
import { GitHubRepository } from "@/types/github";

interface NewWorkspaceDialogProps {
  isOpen: boolean;
  onOpenChange: (open: boolean) => void;
}

const NewWorkspaceDialog = ({
  isOpen,
  onOpenChange,
}: NewWorkspaceDialogProps) => {
  const [newWorkspaceName, setNewWorkspaceName] = useState("");
  const [isCreating, setIsCreating] = useState(false);
  const [githubToken, setGithubToken] = useState("");
  const [selectedRepository, setSelectedRepository] =
    useState<GitHubRepository | null>(null);
  const [selectedBranch, setSelectedBranch] = useState("");

  const createWorkspaceMutation = useCreateWorkspace();

  const handleRepositoryChange = (repository: GitHubRepository | null) => {
    setSelectedRepository(repository);
    setSelectedBranch("");
  };

  const handleCreateWorkspace = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!newWorkspaceName.trim() || !githubToken || !selectedRepository) return;

    setIsCreating(true);
    try {
      const request = {
        name: newWorkspaceName.trim(),
        repo_id: selectedRepository.id,
        token: githubToken,
        branch: selectedBranch || selectedRepository.default_branch || "main",
        provider: "github",
      };

      await createWorkspaceMutation.mutateAsync(request);
      setNewWorkspaceName("");
      setGithubToken("");
      setSelectedRepository(null);
      setSelectedBranch("");
      onOpenChange(false);
      toast.success("Workspace created successfully!");
    } catch (error) {
      toast.error("Failed to create workspace");
      console.error("Error creating workspace:", error);
    } finally {
      setIsCreating(false);
    }
  };

  const isFormValid =
    newWorkspaceName.trim() && githubToken.trim() && selectedRepository;

  return (
    <Dialog open={isOpen} onOpenChange={onOpenChange}>
      <DialogTrigger asChild>
        <Button size={"sm"}>
          <Plus className="h-4 w-4 mr-2" />
          New workspace
        </Button>
      </DialogTrigger>
      <DialogContent className="!max-w-2xl max-h-[90vh] overflow-y-auto">
        <DialogHeader>
          <DialogTitle>Create New Workspace</DialogTitle>
        </DialogHeader>

        <form onSubmit={handleCreateWorkspace} className="space-y-6">
          <div className="space-y-2">
            <Label htmlFor="workspaceName">Name</Label>
            <Input
              id="workspaceName"
              placeholder="Name"
              value={newWorkspaceName}
              onChange={(e) => setNewWorkspaceName(e.target.value)}
              disabled={isCreating}
              autoFocus
            />
          </div>

          <GitHubTokenInput
            token={githubToken}
            onTokenChange={setGithubToken}
            disabled={isCreating}
          />

          <RepositorySelector
            token={githubToken}
            selectedRepository={selectedRepository}
            onRepositoryChange={handleRepositoryChange}
            disabled={isCreating}
          />

          {selectedRepository && (
            <SelectedRepositoryDisplay repository={selectedRepository} />
          )}

          {selectedRepository && (
            <BranchSelector
              token={githubToken}
              selectedBranch={selectedBranch}
              onBranchChange={setSelectedBranch}
              repository={selectedRepository}
              disabled={isCreating}
            />
          )}

          <div className="flex gap-3 justify-end">
            <Button
              type="button"
              variant="outline"
              onClick={() => onOpenChange(false)}
              disabled={isCreating}
            >
              Cancel
            </Button>
            <Button type="submit" disabled={isCreating || !isFormValid}>
              {isCreating ? (
                <Loader2 className="h-4 w-4 animate-spin mr-2" />
              ) : (
                <Plus className="h-4 w-4 mr-2" />
              )}
              Create Workspace
            </Button>
          </div>
        </form>
      </DialogContent>
    </Dialog>
  );
};

export default NewWorkspaceDialog;
