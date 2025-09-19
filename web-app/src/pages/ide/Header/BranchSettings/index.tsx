import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/shadcn/dialog";
import { Separator } from "@/components/ui/shadcn/separator";
import { Github } from "lucide-react";
import SwitchBranch from "./SwitchBranch";
import BranchInfo from "./BranchInfo";

interface BranchSettingsProps {
  isOpen: boolean;
  onClose: () => void;
}

export const BranchSettings = ({ isOpen, onClose }: BranchSettingsProps) => {
  return (
    <Dialog open={isOpen} onOpenChange={onClose}>
      <DialogContent className="max-w-3xl p-0 gap-0">
        <DialogHeader className="p-6">
          <DialogTitle className="flex items-center gap-2 text-lg">
            <Github className="h-5 w-5" />
            Branch Settings
          </DialogTitle>
        </DialogHeader>

        <div className="space-y-6 min-w-0 max-h-[70vh] overflow-y-auto p-6 pt-0 customScrollbar scrollbar-gutter-auto">
          <SwitchBranch />
          <Separator />
          <BranchInfo onFileClick={onClose} />
        </div>
      </DialogContent>
    </Dialog>
  );
};
