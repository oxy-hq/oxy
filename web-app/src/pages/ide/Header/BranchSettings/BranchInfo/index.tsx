import { ContentSkeleton } from "@/components/ui/ContentSkeleton";
import { Badge } from "@/components/ui/shadcn/badge";
import { Label } from "@/components/ui/shadcn/label";
import useRevisionInfo from "@/hooks/api/projects/useRevisionInfo";
import DiffSummary from "@/pages/ide/Header/BranchSettings/BranchInfo/DiffSummary";
import Actions from "./Actions";
import { CommitDisplay } from "./CommitDisplay";

const BranchInfo = ({ onFileClick }: { onFileClick: () => void }) => {
  const { data: revisionInfo, isLoading: revisionLoading } = useRevisionInfo();

  const getSyncStatusBadgeVariant = (status: string) => {
    if (status === "synced") return "secondary";
    if (status === "syncing") return "outline";
    return "destructive";
  };

  if (revisionLoading) {
    return <ContentSkeleton />;
  }

  return (
    <div className='min-w-0 space-y-6'>
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

      <DiffSummary onFileClick={onFileClick} />

      <Actions />
    </div>
  );
};

export default BranchInfo;
