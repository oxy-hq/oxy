import { Label } from "@/components/ui/shadcn/label";
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger
} from "@/components/ui/shadcn/tooltip";

interface CommitDisplayProps {
  // "full-sha - commit message" string from the backend, or a bare SHA
  commit?: string;
  revision?: string;
  label: string;
}

export const CommitDisplay = ({ commit, revision, label }: CommitDisplayProps) => {
  // Parse "sha - message" format from the backend, falling back to bare revision SHA.
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
      <div className='space-y-3'>
        <Label className='font-medium text-sm'>{label}</Label>
        <div>
          <code className='rounded bg-muted px-2 py-1 font-mono text-sm'>None</code>
        </div>
      </div>
    );
  }

  return (
    <div className='space-y-3'>
      <Label className='font-medium text-sm'>{label}</Label>
      <div className='space-y-1'>
        <TooltipProvider>
          <Tooltip>
            <TooltipTrigger asChild>
              <code className='cursor-help rounded bg-muted px-2 py-1 font-mono text-sm'>
                {shortHash}
              </code>
            </TooltipTrigger>
            <TooltipContent>
              <p className='font-mono text-xs'>{sha}</p>
            </TooltipContent>
          </Tooltip>
        </TooltipProvider>
        {message && (
          <TooltipProvider>
            <Tooltip>
              <TooltipTrigger asChild>
                <p className='max-w-[160px] cursor-help truncate text-muted-foreground text-xs'>
                  {message}
                </p>
              </TooltipTrigger>
              <TooltipContent className='max-w-sm'>
                <p className='whitespace-normal break-words text-sm'>{message}</p>
              </TooltipContent>
            </Tooltip>
          </TooltipProvider>
        )}
      </div>
    </div>
  );
};
