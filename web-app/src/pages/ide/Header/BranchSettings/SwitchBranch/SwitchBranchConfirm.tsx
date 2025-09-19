import React from "react";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/shadcn/dialog";
import { Button } from "@/components/ui/shadcn/button";
import { Label } from "@/components/ui/shadcn/label";

interface SwitchBranchConfirmProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  currentBranch: string;
  newBranch: string;
  onConfirm: () => void;
  isLoading?: boolean;
}

const SwitchBranchConfirm: React.FC<SwitchBranchConfirmProps> = ({
  open,
  onOpenChange,
  currentBranch,
  newBranch,
  onConfirm,
  isLoading = false,
}) => {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-[425px]">
        <DialogHeader>
          <DialogTitle>Switch Active Branch</DialogTitle>
          <DialogDescription>
            The most recent changes from the new branch will be synced to this
            chat. This operation is not destructive.
          </DialogDescription>
        </DialogHeader>

        <div className="space-y-4 py-4">
          <div className="space-y-2">
            <Label htmlFor="current-branch">Current branch</Label>
            <div className="px-3 py-2 border rounded-md bg-muted">
              <span className="text-sm font-mono">{currentBranch}</span>
            </div>
          </div>

          <div className="space-y-2">
            <Label htmlFor="new-branch">New branch</Label>
            <div className="px-3 py-2 border rounded-md">
              <span className="text-sm font-mono">{newBranch}</span>
            </div>
          </div>
        </div>

        <DialogFooter>
          <Button
            variant="outline"
            onClick={() => onOpenChange(false)}
            disabled={isLoading}
          >
            Cancel
          </Button>
          <Button onClick={onConfirm} disabled={isLoading}>
            {isLoading ? "Switching..." : "Switch Branch"}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
};

export default SwitchBranchConfirm;
