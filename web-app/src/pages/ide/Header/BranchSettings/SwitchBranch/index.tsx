import { useState } from "react";
import { useNavigate } from "react-router-dom";
import { toast } from "sonner";
import BranchSelector from "@/components/BranchSelector";
import { Label } from "@/components/ui/shadcn/label";
import { useSwitchWorkspaceBranch } from "@/hooks/api/workspaces/useWorkspaces";
import useCurrentWorkspaceBranch from "@/hooks/useCurrentWorkspaceBranch";
import ROUTES from "@/libs/utils/routes";
import useIdeBranch from "@/stores/useIdeBranch";
import SwitchBranchConfirm from "./SwitchBranchConfirm";

const SwitchBranch = () => {
  const navigate = useNavigate();
  const { workspace, branchName: selectedBranch } = useCurrentWorkspaceBranch();
  const { setCurrentBranch } = useIdeBranch();
  const switchBranchMutation = useSwitchWorkspaceBranch();

  const [dialogOpen, setDialogOpen] = useState(false);
  const [pendingBranch, setPendingBranch] = useState<string | null>(null);

  const workspaceId = workspace?.id || "";

  const handleBranchSelect = (branchName: string) => {
    if (branchName === selectedBranch) {
      return;
    }

    setPendingBranch(branchName);
    setDialogOpen(true);
  };

  const handleConfirmSwitch = async () => {
    if (!pendingBranch || !workspaceId) return;

    try {
      await switchBranchMutation.mutateAsync({
        workspaceId: workspaceId,
        branchName: pendingBranch
      });

      setCurrentBranch(workspaceId, pendingBranch);

      toast.success(`Successfully switched to branch "${pendingBranch}"`);

      navigate(ROUTES.WORKSPACE(workspaceId).IDE.ROOT);
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
