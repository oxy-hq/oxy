import { GitBranch, Loader2 } from "lucide-react";
import { Badge } from "@/components/ui/shadcn/badge";
import { Skeleton } from "@/components/ui/shadcn/skeleton";
import useRevisionInfo from "@/hooks/api/projects/useRevisionInfo";
import { CommitShortInfo } from "./CommitShortInfo";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";

export const BranchInfo = () => {
  const { branchName } = useCurrentProjectBranch();
  const { data: revisionInfo, isLoading: revisionLoading } = useRevisionInfo();

  if (revisionLoading) {
    return (
      <div className="flex items-center gap-3 px-3">
        <div className="flex items-center gap-2">
          <Loader2 className="animate-spin h-4 w-4 text-muted-foreground" />
          <Skeleton className="h-5 w-20 rounded" />
        </div>
        <Skeleton className="h-5 w-16 rounded" />
      </div>
    );
  }
  const hasCommitInfo = revisionInfo?.current_commit;

  return (
    <div className="flex items-center gap-3 px-3">
      <Badge
        variant="secondary"
        className="font-mono text-xs cursor-help bg-secondary/50 hover:bg-secondary border-secondary-foreground/20 text-secondary-foreground/80 px-2 py-1"
      >
        <GitBranch className="w-4 h-4 text-muted-foreground flex-shrink-0" />
        {branchName || "No branch"}
      </Badge>

      {hasCommitInfo && (
        <CommitShortInfo
          commit={revisionInfo.current_commit}
          revision={revisionInfo.current_revision}
        />
      )}
    </div>
  );
};
