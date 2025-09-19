import React, { useState } from "react";
import { useParams } from "react-router-dom";
import { useCreateProject } from "@/hooks/api/projects/useProjects";
import { toast } from "sonner";
import { GitHubRepository } from "@/types/github";
import { GitHubTokenInput } from "./GitHubTokenInput";
import { RepositorySelector } from "./RepositorySelector";
import { SelectedRepositoryDisplay } from "./SelectedRepositoryDisplay";
import { BranchSelector } from "./BranchSelector";
import { FormActions } from "./FormActions";

interface NewProjectFormProps {
  onClose: () => void;
}

export const NewProjectForm: React.FC<NewProjectFormProps> = ({ onClose }) => {
  const { organizationId } = useParams<{ organizationId: string }>();
  const [isCreating, setIsCreating] = useState(false);
  const [githubToken, setGithubToken] = useState("");
  const [selectedRepository, setSelectedRepository] =
    useState<GitHubRepository | null>(null);
  const [selectedBranch, setSelectedBranch] = useState("");

  const createProjectMutation = useCreateProject(organizationId!);

  const handleCreateProject = async (e: React.FormEvent) => {
    e.preventDefault();

    if (!githubToken.trim()) {
      toast.error("Please enter a GitHub token");
      return;
    }

    if (!selectedRepository) {
      toast.error("Please select a repository");
      return;
    }

    setIsCreating(true);
    try {
      const data = await createProjectMutation.mutateAsync({
        repo_id: selectedRepository.id,
        token: githubToken,
        branch: selectedBranch || selectedRepository.default_branch || "main",
        provider: "github",
      });
      if (!data.success) {
        toast.error("Failed to create project: " + data.message);
        return;
      }
      setGithubToken("");
      setSelectedRepository(null);
      setSelectedBranch("");
      onClose();
      toast.success("Project created successfully!");
    } catch (error) {
      toast.error("Failed to create project");
      console.error("Error creating project:", error);
    } finally {
      setIsCreating(false);
    }
  };

  const handleRepositoryChange = (repository: GitHubRepository | null) => {
    setSelectedRepository(repository);
    setSelectedBranch(""); // Clear selected branch when repository changes
  };

  const isFormValid = githubToken.trim() && selectedRepository;

  return (
    <form onSubmit={handleCreateProject} className="space-y-4">
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

      <FormActions
        onCancel={onClose}
        isCreating={isCreating}
        isValid={!!isFormValid}
      />
    </form>
  );
};
