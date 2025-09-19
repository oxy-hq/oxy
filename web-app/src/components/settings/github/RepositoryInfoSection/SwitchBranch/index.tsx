import { useState } from "react";
import { toast } from "sonner";
import SwitchBranchConfirm from "./SwitchBranchConfirm";
import useIdeBranch from "@/stores/useIdeBranch";
import BranchSelector from "@/components/BranchSelector";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { useSwitchProjectActiveBranch } from "@/hooks/api/projects/useSwitchProjectActiveBranch";

const SwitchBranch = () => {
  const { project, branchName: selectedBranch } = useCurrentProjectBranch();
  const { setCurrentBranch } = useIdeBranch();
  const switchBranchMutation = useSwitchProjectActiveBranch();

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
        branchName: pendingBranch,
      });

      setCurrentBranch(projectId, pendingBranch);

      toast.success(`Successfully switched to branch "${pendingBranch}"`);

      location.reload();
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
      <BranchSelector
        selectedBranch={selectedBranch}
        setSelectedBranch={handleBranchSelect}
      />

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
