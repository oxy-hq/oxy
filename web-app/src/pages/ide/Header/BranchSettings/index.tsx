import { CreateRepository } from "@/components/CreateRepository";
import GithubIcon from "@/components/ui/GithubIcon";
import { Dialog, DialogContent, DialogHeader, DialogTitle } from "@/components/ui/shadcn/dialog";
import { Separator } from "@/components/ui/shadcn/separator";
import { useAuth } from "@/contexts/AuthContext";
import useCurrentWorkspace from "@/stores/useCurrentWorkspace";
import BranchInfo from "./BranchInfo";
import SwitchBranch from "./SwitchBranch";

interface BranchSettingsProps {
  isOpen: boolean;
  onClose: () => void;
}

export const BranchSettings = ({ isOpen, onClose }: BranchSettingsProps) => {
  const { workspace: project } = useCurrentWorkspace();
  const { authConfig } = useAuth();

  // Show branch UI when local git is enabled OR when a GitHub repo is linked.
  const hasGit = authConfig.local_git || !!project?.project_repo_id;

  return (
    <Dialog open={isOpen} onOpenChange={onClose}>
      <DialogContent className='max-w-3xl gap-0 p-0'>
        <DialogHeader className='p-6'>
          <DialogTitle className='flex items-center gap-2 text-lg'>
            <GithubIcon className='h-5 w-5' />
            {hasGit ? "Branch Settings" : "Connect Repository"}
          </DialogTitle>
        </DialogHeader>

        {hasGit ? (
          <div className='scrollbar-gutter-auto max-h-[70vh] min-w-0 space-y-6 overflow-y-auto p-6 pt-0'>
            <SwitchBranch />
            <Separator />
            <BranchInfo onFileClick={onClose} />
          </div>
        ) : (
          <div className='scrollbar-gutter-auto max-h-[70vh] min-w-0 space-y-6 overflow-y-auto p-6 pt-0'>
            <CreateRepository onSuccess={onClose} />
          </div>
        )}
      </DialogContent>
    </Dialog>
  );
};
