import { useState } from "react";
import { Button } from "@/components/ui/shadcn/button";
import { Input } from "@/components/ui/shadcn/input";
import { Label } from "@/components/ui/shadcn/label";
import { Loader2 } from "lucide-react";
import { useCreateRepoFromProject } from "@/hooks/api/projects";
import useCurrentProject from "@/stores/useCurrentProject";
import { toast } from "sonner";
import { GitNamespaceSelection } from "./GitNamespaceSelection";

interface CreateRepositoryProps {
  onSuccess?: () => void;
}

export const CreateRepository = ({ onSuccess }: CreateRepositoryProps) => {
  const { project } = useCurrentProject();
  const [selectedGitNamespace, setSelectedGitNamespace] = useState<string>("");
  const [repoName, setRepoName] = useState<string>("");

  const createRepoMutation = useCreateRepoFromProject();

  const handleCreateRepo = async () => {
    if (!project?.id || !selectedGitNamespace || !repoName.trim()) {
      toast.error("Please fill in all required fields");
      return;
    }

    createRepoMutation.mutate(
      {
        projectId: project.id,
        gitNamespaceId: selectedGitNamespace,
        repoName: repoName.trim(),
      },
      {
        onSuccess: (response) => {
          if (response.success) {
            toast.success(response.message);
            location.reload();
            onSuccess?.();
          } else {
            toast.error(response.message);
          }
        },
        onError: (error) => {
          toast.error(`Failed to create repository: ${error.message}`);
        },
      },
    );
  };

  const generateDefaultRepoName = () => {
    if (project?.name && !repoName) {
      const sanitized = project.name
        .toLowerCase()
        .replace(/[^a-z0-9]/g, "-")
        .replace(/-{2,}/g, "-");
      const defaultName = sanitized.replace(/^-/, "").replace(/-$/, "");
      setRepoName(defaultName);
    }
  };

  return (
    <div className="flex flex-col gap-6">
      <p className="text-sm text-muted-foreground">
        Create a new{" "}
        <span className="text-blue-600 font-medium">private repository</span> to
        sync changes to. Oxy will push changes to a branch on this repository
        each time you send a message.
      </p>
      <GitNamespaceSelection
        value={selectedGitNamespace}
        onChange={setSelectedGitNamespace}
      />

      <div className="space-y-2">
        <Label htmlFor="repo-name">Repository Name</Label>
        <Input
          id="repo-name"
          value={repoName}
          onChange={(e) => setRepoName(e.target.value)}
          onFocus={generateDefaultRepoName}
          placeholder={
            project?.name
              ? `${project.name.toLowerCase().replace(/[^a-z0-9]/g, "-")}`
              : "repository-name"
          }
          className="font-mono"
        />
      </div>

      <Button
        onClick={handleCreateRepo}
        disabled={
          !selectedGitNamespace ||
          !repoName.trim() ||
          createRepoMutation.isPending
        }
        className="w-full"
      >
        {createRepoMutation.isPending ? (
          <>
            <Loader2 className="h-4 w-4 mr-2 animate-spin" />
            Creating Repository...
          </>
        ) : (
          "Create Repository"
        )}
      </Button>
    </div>
  );
};
