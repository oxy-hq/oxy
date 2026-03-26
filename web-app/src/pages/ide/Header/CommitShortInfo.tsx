import { Badge } from "@/components/ui/shadcn/badge";
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger
} from "@/components/ui/shadcn/tooltip";

interface CommitShortInfoProps {
  commit?: string;
  revision?: string;
}

export const CommitShortInfo = ({ commit, revision }: CommitShortInfoProps) => {
  let sha: string | undefined;
  let message: string | undefined;

  if (commit) {
    const idx = commit.indexOf(" - ");
    if (idx > -1) {
      sha = commit.substring(0, idx);
      message = commit.substring(idx + 3);
    } else {
      sha = commit;
    }
  } else {
    sha = revision;
  }

  const shortHash = sha?.substring(0, 7);

  if (!sha) {
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
            <p className='font-mono text-xs'>{sha}</p>
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
