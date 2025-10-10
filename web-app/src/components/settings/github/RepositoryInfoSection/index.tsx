import { Badge } from "@/components/ui/shadcn/badge";
import { Label } from "@/components/ui/shadcn/label";
import { Skeleton } from "@/components/ui/shadcn/skeleton";
import { Loader2 } from "lucide-react";
import { CommitDisplay } from "./CommitDisplay";
import useRevisionInfo from "@/hooks/api/projects/useRevisionInfo";
import useCurrentProject from "@/stores/useCurrentProject";
import { Button } from "@/components/ui/shadcn/button";
import { SyncDialog } from "./SyncDialog";
import { useState } from "react";
import SwitchBranch from "./SwitchBranch";

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
    return (
      <div className="space-y-4">
        <div className="flex items-center gap-2">
          <Loader2 className="h-4 w-4 animate-spin" />
          <span className="text-sm text-muted-foreground">
            Loading repository info...
          </span>
        </div>
        <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
          <div className="space-y-2">
            <Skeleton className="h-4 w-24" />
            <Skeleton className="h-8 w-20" />
            <Skeleton className="h-3 w-48" />
          </div>
          <div className="space-y-2">
            <Skeleton className="h-4 w-24" />
            <Skeleton className="h-8 w-20" />
            <Skeleton className="h-3 w-48" />
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className="space-y-6">
      <div>
        <Label className="text-sm font-medium">Repository</Label>
        <div className="mt-2">
          <code className="px-2 py-1 bg-muted rounded text-sm font-mono cursor-help">
            {project?.name}
          </code>
        </div>
      </div>
      <div>
        <Label className="text-sm font-medium">Active branch</Label>
        <div className="mt-2">
          <SwitchBranch />
        </div>
      </div>
      <div>
        <Label className="text-sm font-medium">Sync Status</Label>
        <div className="mt-2">
          <Badge
            variant={getSyncStatusBadgeVariant(
              revisionInfo?.sync_status || "idle",
            )}
          >
            {revisionInfo?.sync_status
              ? revisionInfo.sync_status.charAt(0).toUpperCase() +
                revisionInfo.sync_status.slice(1)
              : "Idle"}
          </Badge>
        </div>
      </div>

      <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
        <CommitDisplay
          commit={revisionInfo?.current_commit}
          revision={revisionInfo?.current_revision}
          label="Current Revision"
        />
        <CommitDisplay
          commit={revisionInfo?.latest_commit}
          revision={revisionInfo?.latest_revision}
          label="Latest Revision"
        />
      </div>

      {revisionInfo?.last_sync_time && (
        <div>
          <Label className="text-sm font-medium">Last Synced</Label>
          <p className="text-sm text-muted-foreground mt-1">
            {new Date(revisionInfo.last_sync_time).toLocaleString()}
          </p>
        </div>
      )}

      {revisionInfo?.current_revision !== revisionInfo?.latest_revision && (
        <Button size="sm" onClick={() => setSyncDialogOpen(true)}>
          Sync Now
        </Button>
      )}
      <SyncDialog open={syncDialogOpen} onOpenChange={setSyncDialogOpen} />
    </div>
  );
};

export default RepositoryInfoSection;
