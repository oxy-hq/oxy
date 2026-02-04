import { GitBranch, Loader2 } from "lucide-react";
import { Badge } from "@/components/ui/shadcn/badge";
import { Skeleton } from "@/components/ui/shadcn/skeleton";
import useRevisionInfo from "@/hooks/api/projects/useRevisionInfo";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { CommitShortInfo } from "./CommitShortInfo";

export const BranchInfo = () => {
  const { branchName } = useCurrentProjectBranch();
  const { data: revisionInfo, isLoading: revisionLoading } = useRevisionInfo();

  if (revisionLoading) {
    return (
      <div className='flex items-center gap-3 px-3'>
        <div className='flex items-center gap-2'>
          <Loader2 className='h-4 w-4 animate-spin text-muted-foreground' />
          <Skeleton className='h-5 w-20 rounded' />
        </div>
        <Skeleton className='h-5 w-16 rounded' />
      </div>
    );
  }
  const hasCommitInfo = revisionInfo?.current_commit;

  return (
    <div className='flex items-center gap-3 px-3'>
      <Badge
        variant='secondary'
        className='cursor-help border-secondary-foreground/20 bg-secondary/50 px-2 py-1 font-mono text-secondary-foreground/80 text-xs hover:bg-secondary'
      >
        <GitBranch className='h-4 w-4 flex-shrink-0 text-muted-foreground' />
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
