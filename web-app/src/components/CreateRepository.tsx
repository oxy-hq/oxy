import { useState } from "react";
import { toast } from "sonner";
import { Button } from "@/components/ui/shadcn/button";
import { Input } from "@/components/ui/shadcn/input";
import { Label } from "@/components/ui/shadcn/label";
import { Spinner } from "@/components/ui/shadcn/spinner";
import { useCreateRepoFromWorkspace } from "@/hooks/api/workspaces/useCreateRepoFromWorkspace";
import useCurrentWorkspace from "@/stores/useCurrentWorkspace";
import { GitNamespaceSelection } from "./GitNamespaceSelection";

interface CreateRepositoryProps {
  onSuccess?: () => void;
}

export const CreateRepository = ({ onSuccess }: CreateRepositoryProps) => {
  const { workspace } = useCurrentWorkspace();
  const [selectedGitNamespace, setSelectedGitNamespace] = useState<string>("");
  const [repoName, setRepoName] = useState<string>("");

  const createRepoMutation = useCreateRepoFromWorkspace();

  const handleCreateRepo = async () => {
    if (!workspace?.id || !selectedGitNamespace || !repoName.trim()) {
      toast.error("Please fill in all required fields");
      return;
    }

    createRepoMutation.mutate(
      {
        workspaceId: workspace.id,
        gitNamespaceId: selectedGitNamespace,
        repoName: repoName.trim()
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
        }
      }
    );
  };

  const generateDefaultRepoName = () => {
    if (workspace?.name && !repoName) {
      const sanitized = workspace.name
        .toLowerCase()
        .replace(/[^a-z0-9]/g, "-")
        .replace(/-{2,}/g, "-");
      const defaultName = sanitized.replace(/^-/, "").replace(/-$/, "");
      setRepoName(defaultName);
    }
  };

  return (
    <div className='flex flex-col gap-6'>
      <p className='text-muted-foreground text-sm'>
        Create a new <span className='font-medium text-info'>private repository</span> to sync
        changes to. Oxy will push changes to a branch on this repository each time you send a
        message.
      </p>
      <GitNamespaceSelection value={selectedGitNamespace} onChange={setSelectedGitNamespace} />

      <div className='space-y-2'>
        <Label htmlFor='repo-name'>Repository Name</Label>
        <Input
          id='repo-name'
          value={repoName}
          onChange={(e) => setRepoName(e.target.value)}
          onFocus={generateDefaultRepoName}
          placeholder={
            workspace?.name
              ? `${workspace.name.toLowerCase().replace(/[^a-z0-9]/g, "-")}`
              : "repository-name"
          }
          className='font-mono'
        />
      </div>

      <Button
        onClick={handleCreateRepo}
        disabled={!selectedGitNamespace || !repoName.trim() || createRepoMutation.isPending}
        className='w-full'
      >
        {createRepoMutation.isPending ? <Spinner className='mr-2' /> : "Create Repository"}
      </Button>
    </div>
  );
};
