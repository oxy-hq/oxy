import { Loader2, AlertCircle } from "lucide-react";
import { Badge } from "@/components/ui/shadcn/badge";
import { Combobox } from "@/components/ui/shadcn/combobox";
import { Alert, AlertDescription } from "@/components/ui/shadcn/alert";
import { useProjectBranches } from "@/hooks/api/projects/useProjects";
import useCurrentProject from "@/stores/useCurrentProject";

interface Props {
  selectedBranch: string;
  setSelectedBranch: (branch: string) => void;
}

const BranchSelector = ({ selectedBranch, setSelectedBranch }: Props) => {
  const { project } = useCurrentProject();
  const {
    data: branchResponse,
    isLoading,
    error,
  } = useProjectBranches(project?.id || "");

  const branches = branchResponse?.branches || [];
  const activeBranchName = project?.active_branch?.name;

  if (isLoading) {
    return (
      <div className="flex items-center gap-2 p-3 border rounded-md bg-muted/30">
        <Loader2 className="h-4 w-4 animate-spin text-muted-foreground" />
        <span className="text-sm text-muted-foreground">
          Loading branches...
        </span>
      </div>
    );
  }

  if (error) {
    return (
      <Alert variant="destructive">
        <AlertCircle className="h-4 w-4" />
        <AlertDescription>
          Failed to load branches. Please try again later.
        </AlertDescription>
      </Alert>
    );
  }

  if (branches.length === 0) {
    return (
      <Alert>
        <AlertCircle className="h-4 w-4" />
        <AlertDescription>No branches found for this project.</AlertDescription>
      </Alert>
    );
  }

  return (
    <Combobox
      items={branches.map((branch) => ({
        value: branch.name,
        label: branch.name,
        searchText: branch.name.toLowerCase(),
      }))}
      value={selectedBranch}
      onValueChange={setSelectedBranch}
      placeholder="Select a branch"
      searchPlaceholder="Search branches..."
      renderItem={(item) => (
        <div className="flex items-center justify-between w-full">
          <div className="flex items-center gap-2">
            <span className="text-sm font-medium">{item.label}</span>
            <div className="flex gap-1">
              {item.value === activeBranchName && (
                <Badge variant="secondary" className="text-xs px-1.5 py-0.5">
                  active
                </Badge>
              )}
              {item.value === selectedBranch &&
                item.value !== activeBranchName && (
                  <Badge
                    variant="outline"
                    className="text-xs px-1.5 py-0.5 bg-blue-50 text-blue-700 border-blue-200 dark:bg-blue-900/20 dark:text-blue-400 dark:border-blue-800"
                  >
                    current
                  </Badge>
                )}
            </div>
          </div>
        </div>
      )}
    />
  );
};

export default BranchSelector;
