import { Badge } from "@/components/ui/shadcn/badge";
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger
} from "@/components/ui/shadcn/tooltip";
import type { CommitInfo } from "@/types/settings";

interface CommitShortInfoProps {
  commit?: CommitInfo;
  revision?: string;
}

export const CommitShortInfo = ({ commit, revision }: CommitShortInfoProps) => {
  const commitHash = commit?.sha || revision;
  const shortHash = commitHash?.substring(0, 7);
  const message = commit?.message;

  if (!commitHash) {
    return (
      <Badge variant='outline' className='text-muted-foreground'>
        No commit
      </Badge>
    );
  }

  return (
    <div className='flex items-center gap-2'>
      <TooltipProvider>
        <Tooltip>
          <TooltipTrigger asChild>
            <Badge
              variant='outline'
              className='cursor-help border-secondary-foreground/20 bg-secondary/50 px-2 py-1 font-mono text-secondary-foreground/80 text-xs hover:bg-secondary'
            >
              {shortHash}
            </Badge>
          </TooltipTrigger>
          <TooltipContent side='bottom'>
            <p className='font-mono text-xs'>{commitHash}</p>
          </TooltipContent>
        </Tooltip>
      </TooltipProvider>

      {message && (
        <TooltipProvider>
          <Tooltip>
            <TooltipTrigger asChild>
              <span className='max-w-32 cursor-help truncate text-muted-foreground text-sm transition-colors hover:text-foreground'>
                {message}
              </span>
            </TooltipTrigger>
            <TooltipContent className='max-w-sm' side='bottom'>
              <p className='whitespace-normal break-words text-sm'>{message}</p>
            </TooltipContent>
          </Tooltip>
        </TooltipProvider>
      )}
    </div>
  );
};
