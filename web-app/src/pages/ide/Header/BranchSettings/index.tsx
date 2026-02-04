import { Github } from "lucide-react";
import { CreateRepository } from "@/components/CreateRepository";
import { Dialog, DialogContent, DialogHeader, DialogTitle } from "@/components/ui/shadcn/dialog";
import { Separator } from "@/components/ui/shadcn/separator";
import useCurrentProject from "@/stores/useCurrentProject";
import BranchInfo from "./BranchInfo";
import SwitchBranch from "./SwitchBranch";

interface BranchSettingsProps {
  isOpen: boolean;
  onClose: () => void;
}

export const BranchSettings = ({ isOpen, onClose }: BranchSettingsProps) => {
  const { project } = useCurrentProject();

  const project_repo_id = project?.project_repo_id;
  return (
    <Dialog open={isOpen} onOpenChange={onClose}>
      <DialogContent className='max-w-3xl gap-0 p-0'>
        <DialogHeader className='p-6'>
          <DialogTitle className='flex items-center gap-2 text-lg'>
            {project_repo_id ? (
              <>
                <Github className='h-5 w-5' />
                Branch Settings
              </>
            ) : (
              <>
                <Github className='h-5 w-5' />
                Connect Repository
              </>
            )}
          </DialogTitle>
        </DialogHeader>

        {project_repo_id ? (
          <div className='customScrollbar scrollbar-gutter-auto max-h-[70vh] min-w-0 space-y-6 overflow-y-auto p-6 pt-0'>
            <SwitchBranch />
            <Separator />
            <BranchInfo onFileClick={onClose} />
          </div>
        ) : (
          <div className='customScrollbar scrollbar-gutter-auto max-h-[70vh] min-w-0 space-y-6 overflow-y-auto p-6 pt-0'>
            <CreateRepository onSuccess={onClose} />
          </div>
        )}
      </DialogContent>
    </Dialog>
  );
};
