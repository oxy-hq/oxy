import { GitBranch, Loader2 } from "lucide-react";
import { Skeleton } from "@/components/ui/shadcn/skeleton";
import useRevisionInfo from "@/hooks/api/projects/useRevisionInfo";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";

export const BranchInfo = () => {
  const { branchName } = useCurrentProjectBranch();
  const { isLoading: revisionLoading } = useRevisionInfo();

  if (revisionLoading) {
    return (
      <div className='flex items-center gap-2'>
        <Loader2 className='h-3.5 w-3.5 animate-spin text-muted-foreground' />
        <Skeleton className='h-4 w-20 rounded' />
      </div>
    );
  }

  return (
    <div className='flex min-w-0 items-center gap-2'>
      <GitBranch className='h-3.5 w-3.5 flex-shrink-0 text-muted-foreground' />
      <span className='truncate font-mono text-sm'>{branchName || "No branch"}</span>
    </div>
  );
};
