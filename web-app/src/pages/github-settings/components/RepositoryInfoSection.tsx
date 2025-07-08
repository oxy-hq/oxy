import { Button } from "@/components/ui/shadcn/button";
import { Label } from "@/components/ui/shadcn/label";
import { Badge } from "@/components/ui/shadcn/badge";
import { Loader2, GitBranch } from "lucide-react";
import { useSyncGitHubRepository } from "@/hooks/api/useGithubSettings";
import { CommitDisplay } from "./CommitDisplay";
import { CommitInfo } from "@/types/settings";

interface RevisionInfo {
  sync_status?: string;
  current_revision?: string;
  latest_revision?: string;
  current_commit?: CommitInfo;
  latest_commit?: CommitInfo;
  last_sync_time?: string;
}

interface RepositoryInfoSectionProps {
  repositoryName?: string;
  revisionInfo?: RevisionInfo;
  revisionLoading: boolean;
}

export const RepositoryInfoSection = ({
  repositoryName,
  revisionInfo,
  revisionLoading,
}: RepositoryInfoSectionProps) => {
  const syncRepositoryMutation = useSyncGitHubRepository();

  const getSyncStatusBadgeVariant = (status: string) => {
    if (status === "synced") return "secondary";
    if (status === "syncing") return "outline";
    return "destructive";
  };

  const syncRepository = async () => {
    await syncRepositoryMutation.mutateAsync();
  };

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <div className="space-y-1">
          <Label className="text-sm font-medium">Repository Information</Label>
          <p className="text-sm text-muted-foreground">
            {repositoryName || "Selected repository"}
          </p>
        </div>
        <div className="flex items-center gap-2">
          {revisionLoading ? (
            <div className="flex items-center gap-2">
              <Loader2 className="animate-spin h-4 w-4" />
              <span className="text-sm text-muted-foreground">Loading...</span>
            </div>
          ) : (
            <>
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
              {/* Show sync button if sync is in progress */}
              {revisionInfo?.sync_status === "syncing" && (
                <Button
                  onClick={syncRepository}
                  disabled={
                    syncRepositoryMutation.isPending ||
                    revisionInfo?.sync_status === "syncing"
                  }
                  size="sm"
                >
                  {syncRepositoryMutation.isPending ? (
                    <Loader2 className="animate-spin h-4 w-4 mr-2" />
                  ) : (
                    <GitBranch className="h-4 w-4 mr-2" />
                  )}
                  Syncing...
                </Button>
              )}
              {/* Show sync button for manual sync when not syncing and current revision is not the latest */}
              {revisionInfo?.sync_status !== "syncing" &&
                revisionInfo?.current_revision !==
                  revisionInfo?.latest_revision && (
                  <Button
                    onClick={syncRepository}
                    disabled={syncRepositoryMutation.isPending}
                    size="sm"
                    variant="outline"
                  >
                    {syncRepositoryMutation.isPending ? (
                      <Loader2 className="animate-spin h-4 w-4 mr-2" />
                    ) : (
                      <GitBranch className="h-4 w-4 mr-2" />
                    )}
                    Sync
                  </Button>
                )}
            </>
          )}
        </div>
      </div>

      {revisionLoading ? (
        <div className="space-y-4">
          <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
            {/* Loading skeletons for commit displays */}
            <div className="space-y-2">
              <div className="h-4 w-24 bg-muted rounded animate-pulse" />
              <div className="h-8 w-16 bg-muted rounded animate-pulse" />
              <div className="h-3 w-48 bg-muted rounded animate-pulse" />
            </div>
            <div className="space-y-2">
              <div className="h-4 w-24 bg-muted rounded animate-pulse" />
              <div className="h-8 w-16 bg-muted rounded animate-pulse" />
              <div className="h-3 w-48 bg-muted rounded animate-pulse" />
            </div>
          </div>
        </div>
      ) : (
        <>
          <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
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
            <p className="text-sm text-muted-foreground">
              Last synced:{" "}
              {new Date(revisionInfo.last_sync_time).toLocaleString()}
            </p>
          )}
        </>
      )}
    </div>
  );
};
