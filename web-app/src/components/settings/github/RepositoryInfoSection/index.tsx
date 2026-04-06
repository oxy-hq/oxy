import { useState } from "react";
import LoadingSkeleton from "@/components/ui/LoadingSkeleton";
import { Badge } from "@/components/ui/shadcn/badge";
import { Button } from "@/components/ui/shadcn/button";
import { Label } from "@/components/ui/shadcn/label";
import useRevisionInfo from "@/hooks/api/projects/useRevisionInfo";
import useCurrentProject from "@/stores/useCurrentProject";
import { CommitDisplay } from "./CommitDisplay";
import SwitchBranch from "./SwitchBranch";
import { SyncDialog } from "./SyncDialog";

const RepositoryInfoSection = () => {
  const { project } = useCurrentProject();
  const [syncDialogOpen, setSyncDialogOpen] = useState(false);
  const { data: revisionInfo, isLoading: revisionLoading } = useRevisionInfo();

  const getSyncStatusBadgeVariant = (status: string) => {
    if (status === "synced") return "secondary";
    if (status === "syncing") return "outline";
    return "destructive";
  };

  if (revisionLoading) {
    return <LoadingSkeleton />;
  }

  return (
    <div className='space-y-6'>
      <div>
        <Label className='font-medium text-sm'>Repository</Label>
        <div className='mt-2'>
          <code className='cursor-help rounded bg-muted px-2 py-1 font-mono text-sm'>
            {project?.name}
          </code>
        </div>
      </div>
      <div>
        <Label className='font-medium text-sm'>Active branch</Label>
        <div className='mt-2'>
          <SwitchBranch />
        </div>
      </div>
      <div>
        <Label className='font-medium text-sm'>Sync Status</Label>
        <div className='mt-2'>
          <Badge variant={getSyncStatusBadgeVariant(revisionInfo?.sync_status || "idle")}>
            {revisionInfo?.sync_status
              ? revisionInfo.sync_status.charAt(0).toUpperCase() + revisionInfo.sync_status.slice(1)
              : "Idle"}
          </Badge>
        </div>
      </div>

      <div className='grid grid-cols-1 gap-6 md:grid-cols-2'>
        <CommitDisplay
          commit={revisionInfo?.current_commit}
          revision={revisionInfo?.current_revision}
          label='Current Revision'
        />
        <CommitDisplay
          commit={revisionInfo?.latest_commit}
          revision={revisionInfo?.latest_revision}
          label='Latest Revision'
        />
      </div>

      {revisionInfo?.last_sync_time && (
        <div>
          <Label className='font-medium text-sm'>Last Synced</Label>
          <p className='mt-1 text-muted-foreground text-sm'>
            {new Date(revisionInfo.last_sync_time).toLocaleString()}
          </p>
        </div>
      )}

      {revisionInfo?.current_revision !== revisionInfo?.latest_revision && (
        <Button size='sm' onClick={() => setSyncDialogOpen(true)}>
          Sync Now
        </Button>
      )}
      <SyncDialog open={syncDialogOpen} onOpenChange={setSyncDialogOpen} />
    </div>
  );
};

export default RepositoryInfoSection;
