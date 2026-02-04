import { useState } from "react";
import { useNavigate } from "react-router-dom";
import { toast } from "sonner";
import BranchSelector from "@/components/BranchSelector";
import { Label } from "@/components/ui/shadcn/label";
import { useSwitchProjectBranch } from "@/hooks/api/projects/useProjects";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import ROUTES from "@/libs/utils/routes";
import useIdeBranch from "@/stores/useIdeBranch";
import SwitchBranchConfirm from "./SwitchBranchConfirm";

const SwitchBranch = () => {
  const navigate = useNavigate();
  const { project, branchName: selectedBranch } = useCurrentProjectBranch();
  const { setCurrentBranch } = useIdeBranch();
  const switchBranchMutation = useSwitchProjectBranch();

  const [dialogOpen, setDialogOpen] = useState(false);
  const [pendingBranch, setPendingBranch] = useState<string | null>(null);

  const projectId = project?.id || "";

  const handleBranchSelect = (branchName: string) => {
    if (branchName === selectedBranch) {
      return;
    }

    setPendingBranch(branchName);
    setDialogOpen(true);
  };

  const handleConfirmSwitch = async () => {
    if (!pendingBranch || !projectId) return;

    try {
      await switchBranchMutation.mutateAsync({
        projectId: projectId,
        branchName: pendingBranch
      });

      setCurrentBranch(projectId, pendingBranch);

      toast.success(`Successfully switched to branch "${pendingBranch}"`);

      navigate(ROUTES.PROJECT(projectId).IDE.ROOT);
    } catch (error) {
      console.error("Failed to switch branch:", error);
      toast.error("Failed to switch branch. Please try again.");
    } finally {
      setDialogOpen(false);
      setPendingBranch(null);
    }
  };

  return (
    <>
      <div>
        <Label className='pb-2 font-medium text-sm'>Current branch</Label>
        <BranchSelector selectedBranch={selectedBranch} setSelectedBranch={handleBranchSelect} />
      </div>

      <SwitchBranchConfirm
        open={dialogOpen}
        onOpenChange={setDialogOpen}
        currentBranch={selectedBranch}
        newBranch={pendingBranch || ""}
        onConfirm={handleConfirmSwitch}
        isLoading={switchBranchMutation.isPending}
      />
    </>
  );
};

export default SwitchBranch;
