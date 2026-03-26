import { useState } from "react";
import { Button } from "@/components/ui/shadcn/button";
import { useAuth } from "@/contexts/AuthContext";
import { PullDialog } from "./PullDialog";
import { PushDialog } from "./PushDialog";

const Actions = () => {
  const { authConfig } = useAuth();
  const [pullDialogOpen, setPullDialogOpen] = useState(false);
  const [pushDialogOpen, setPushDialogOpen] = useState(false);

  const isLocalOnly = !!authConfig.local_git && !authConfig.cloud;
  // Show Pull when using cloud GitHub integration OR when a remote is configured locally.
  const showPull = authConfig.cloud || authConfig.git_remote;
  const pushLabel = isLocalOnly
    ? authConfig.git_remote
      ? "Commit & Push"
      : "Commit changes"
    : "Push changes";

  return (
    <>
      <div className='flex gap-2'>
        {showPull && (
          <Button variant='outline' className='flex-1' onClick={() => setPullDialogOpen(true)}>
            Pull changes
          </Button>
        )}
        <Button variant='outline' className='flex-1' onClick={() => setPushDialogOpen(true)}>
          {pushLabel}
        </Button>
      </div>

      {showPull && <PullDialog open={pullDialogOpen} onOpenChange={setPullDialogOpen} />}

      <PushDialog open={pushDialogOpen} onOpenChange={setPushDialogOpen} />
    </>
  );
};

export default Actions;
