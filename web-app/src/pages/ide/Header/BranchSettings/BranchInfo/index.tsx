import LoadingSkeleton from "@/components/ui/LoadingSkeleton";
import { Label } from "@/components/ui/shadcn/label";
import { useAuth } from "@/contexts/AuthContext";
import useRevisionInfo from "@/hooks/api/projects/useRevisionInfo";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import DiffSummary from "@/pages/ide/Header/BranchSettings/BranchInfo/DiffSummary";
import { SyncStatusBadge } from "@/pages/ide/Header/BranchSettings/SyncStatusBadge";
import Actions from "./Actions";
import { CommitDisplay } from "./CommitDisplay";
import ConflictPanel from "./ConflictPanel";

const BranchInfo = ({ onFileClick }: { onFileClick: () => void }) => {
  const { authConfig } = useAuth();
  const { branchName } = useCurrentProjectBranch();
  const { data: revisionInfo, isLoading: revisionLoading } = useRevisionInfo();

  if (revisionLoading) {
    return <LoadingSkeleton />;
  }

  // latest_revision is empty when there is no remote configured — hide the section.
  const hasRemoteRevision = authConfig.local_git && !!revisionInfo?.latest_revision;

  // In local mode the server now computes sync_status using git ancestry
  // (git rev-list --count HEAD..{remote_sha}) so raw SHA comparison is not needed.
  // After a pull --rebase the rebased local commit has a new SHA but is still
  // "up to date" — ancestry check catches this correctly.
  const showSyncStatus = authConfig.local_git ? hasRemoteRevision : true;
  const syncStatus = revisionInfo?.sync_status ?? "idle";

  return (
    <div className='min-w-0 space-y-6'>
      {showSyncStatus && (
        <div>
          <Label className='font-medium text-sm'>Sync Status</Label>
          <div className='mt-2'>
            <SyncStatusBadge status={syncStatus} />
          </div>
        </div>
      )}

      {syncStatus === "conflict" && (
        <ConflictPanel remoteUrl={revisionInfo?.remote_url} branch={branchName} />
      )}

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
