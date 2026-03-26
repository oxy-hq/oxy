import { Label } from "@/components/ui/shadcn/label";
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger
} from "@/components/ui/shadcn/tooltip";

interface CommitDisplayProps {
  commit?: string;
  revision?: string;
  label: string;
}

export const CommitDisplay = ({ commit, revision, label }: CommitDisplayProps) => {
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
  } else if (revision) {
    sha = revision;
  }

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
                {sha.substring(0, 7)}
              </code>
            </TooltipTrigger>
            <TooltipContent>
              <p className='font-mono'>{sha}</p>
            </TooltipContent>
          </Tooltip>
        </TooltipProvider>
        {message && <p className='truncate text-muted-foreground text-xs'>{message}</p>}
      </div>
    </div>
  );
};
