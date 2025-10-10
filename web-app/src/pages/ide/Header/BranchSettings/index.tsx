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
import { CreateRepository } from "@/components/CreateRepository";
import useCurrentProject from "@/stores/useCurrentProject";

interface BranchSettingsProps {
  isOpen: boolean;
  onClose: () => void;
}

export const BranchSettings = ({ isOpen, onClose }: BranchSettingsProps) => {
  const { project } = useCurrentProject();

  const project_repo_id = project?.project_repo_id;
  return (
    <Dialog open={isOpen} onOpenChange={onClose}>
      <DialogContent className="max-w-3xl p-0 gap-0">
        <DialogHeader className="p-6">
          <DialogTitle className="flex items-center gap-2 text-lg">
            {project_repo_id ? (
              <>
                <Github className="h-5 w-5" />
                Branch Settings
              </>
            ) : (
              <>
                <Github className="h-5 w-5" />
                Connect Repository
              </>
            )}
          </DialogTitle>
        </DialogHeader>

        {project_repo_id ? (
          <div className="space-y-6 min-w-0 max-h-[70vh] overflow-y-auto p-6 pt-0 customScrollbar scrollbar-gutter-auto">
            <SwitchBranch />
            <Separator />
            <BranchInfo onFileClick={onClose} />
          </div>
        ) : (
          <div className="space-y-6 min-w-0 max-h-[70vh] overflow-y-auto p-6 pt-0 customScrollbar scrollbar-gutter-auto">
            <CreateRepository onSuccess={onClose} />
          </div>
        )}
      </DialogContent>
    </Dialog>
  );
};
