import type { ReactNode } from "react";
import { BranchPopover } from "./BranchPopover";
import type { BranchRowData } from "./BranchPopover/BranchRow";

interface Props {
  trigger: ReactNode;
  open?: boolean;
  onOpenChange?: (open: boolean) => void;
  branches: string[];
  currentBranch: string | undefined;
  isLoading: boolean;
  onSelect: (branchName: string) => void;
}

export function RepoBranchSwitcher({
  trigger,
  open,
  onOpenChange,
  branches,
  currentBranch,
  isLoading,
  onSelect
}: Props) {
  const rows: BranchRowData[] = branches.map((name) => ({ name }));

  return (
    <BranchPopover
      trigger={trigger}
      open={open}
      onOpenChange={onOpenChange}
      branches={rows}
      activeBranch={currentBranch}
      switchingTo={null}
      isLoading={isLoading}
      onSelect={(name) => {
        onOpenChange?.(false);
        onSelect(name);
      }}
    />
  );
}
