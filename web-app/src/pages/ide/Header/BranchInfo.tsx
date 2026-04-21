import { GitBranch } from "lucide-react";
import { Skeleton } from "@/components/ui/shadcn/skeleton";
import { Spinner } from "@/components/ui/shadcn/spinner";
import { useAuth } from "@/contexts/AuthContext";
import useRevisionInfo from "@/hooks/api/workspaces/useRevisionInfo";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";

export const BranchInfo = () => {
  const { isLocalMode } = useAuth();
  const { branchName } = useCurrentProjectBranch();
  const { isLoading: revisionLoading } = useRevisionInfo(!isLocalMode);

  if (isLocalMode) return null;

  if (revisionLoading) {
    return (
      <div className='flex items-center gap-2'>
        <Spinner className='size-3 text-muted-foreground' />
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
