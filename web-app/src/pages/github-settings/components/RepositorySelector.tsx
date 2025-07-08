import { useState } from "react";
import { Button } from "@/components/ui/shadcn/button";
import { Label } from "@/components/ui/shadcn/label";
import { Badge } from "@/components/ui/shadcn/badge";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/shadcn/dialog";
import { Loader2, CheckCircle } from "lucide-react";
import { Combobox } from "@/components/ui/shadcn/combobox";
import {
  useListRepositories,
  useSelectRepository,
} from "@/hooks/api/useGithubSettings";

interface RepositorySelectorProps {
  isTokenConfigured: boolean;
  selectedRepoId?: number;
}

export const RepositorySelector = ({
  isTokenConfigured,
  selectedRepoId,
}: RepositorySelectorProps) => {
  const {
    data: repositories = [],
    isLoading: repositoriesLoading,
    refetch: refetchRepositories,
  } = useListRepositories();
  const selectRepositoryMutation = useSelectRepository();

  const [showRepositoryModal, setShowRepositoryModal] = useState(false);
  const [localSelectedRepoId, setLocalSelectedRepoId] = useState<string>("");

  const handleRepositoryChange = async (value: string) => {
    if (!value) return;
    setLocalSelectedRepoId(value);
  };

  const confirmRepositorySelection = async () => {
    if (!localSelectedRepoId) return;

    const repositoryId = parseInt(localSelectedRepoId, 10);
    await selectRepositoryMutation.mutateAsync(repositoryId);
    setShowRepositoryModal(false);
    setLocalSelectedRepoId("");
  };

  const handleModalOpen = (open: boolean) => {
    setShowRepositoryModal(open);
    if (open && isTokenConfigured) {
      // Auto-load repositories when modal opens
      refetchRepositories();
      // Initialize selected repo ID with current selection
      setLocalSelectedRepoId(selectedRepoId?.toString() || "");
    } else if (!open) {
      // Reset state when closing
      setLocalSelectedRepoId("");
    }
  };

  return (
    <div className="flex items-center justify-between">
      <div className="flex">
        <div className="space-y-1">
          <Label className="text-sm font-medium">Selected Repository</Label>
          <p className="text-sm text-muted-foreground">
            Currently active GitHub repository
          </p>
        </div>
      </div>

      <div className="flex items-center gap-2">
        {selectedRepoId ? (
          <Badge variant="secondary" className="flex items-center gap-1">
            <CheckCircle className="h-3 w-3" />
            Repository Selected
          </Badge>
        ) : (
          <Badge variant="outline">No Repository Selected</Badge>
        )}

        {isTokenConfigured && (
          <Dialog open={showRepositoryModal} onOpenChange={handleModalOpen}>
            <DialogTrigger asChild>
              <Button
                variant="outline"
                size="sm"
                onClick={() => setShowRepositoryModal(true)}
              >
                {selectedRepoId ? "Change" : "Select"} Repository
              </Button>
            </DialogTrigger>
            <DialogContent className="sm:max-w-md">
              <DialogHeader>
                <DialogTitle>Select Repository</DialogTitle>
                <DialogDescription>
                  Choose a repository from your GitHub account to work with.
                </DialogDescription>
              </DialogHeader>

              <div className="space-y-4">
                <div className="space-y-2">
                  <Label>Repository</Label>
                  <Combobox
                    items={repositories.map((repo) => ({
                      value: repo.id.toString(),
                      label: repo.full_name,
                      searchText: `${repo.full_name} ${repo.name} ${
                        repo.description || ""
                      }`.toLowerCase(),
                    }))}
                    value={localSelectedRepoId}
                    onValueChange={handleRepositoryChange}
                    placeholder={
                      repositoriesLoading
                        ? "Loading repositories..."
                        : "Select a repository"
                    }
                    searchPlaceholder="Search repositories..."
                    disabled={
                      repositoriesLoading || selectRepositoryMutation.isPending
                    }
                    className="w-full"
                    renderItem={(item) => (
                      <div className="flex items-center justify-between w-full">
                        <div className="flex-1">
                          <div className="text-sm font-medium">
                            {item.label}
                          </div>
                        </div>
                      </div>
                    )}
                  />
                </div>
              </div>

              <DialogFooter>
                <Button
                  variant="outline"
                  onClick={() => setShowRepositoryModal(false)}
                >
                  Cancel
                </Button>
                <Button
                  onClick={confirmRepositorySelection}
                  disabled={
                    !localSelectedRepoId ||
                    repositoriesLoading ||
                    selectRepositoryMutation.isPending
                  }
                >
                  {selectRepositoryMutation.isPending && (
                    <Loader2 className="animate-spin h-4 w-4 mr-2" />
                  )}
                  Select Repository
                </Button>
              </DialogFooter>
            </DialogContent>
          </Dialog>
        )}
      </div>
    </div>
  );
};
