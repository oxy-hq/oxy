import { Calendar, MessageSquare, User } from "lucide-react";
import { Label } from "@/components/ui/shadcn/label";
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger
} from "@/components/ui/shadcn/tooltip";
import type { CommitInfo } from "@/types/settings";

interface CommitDisplayProps {
  commit?: CommitInfo;
  revision?: string;
  label: string;
}

export const CommitDisplay = ({ commit, revision, label }: CommitDisplayProps) => {
  if (commit) {
    return (
      <div className='space-y-3'>
        <Label className='font-medium text-sm'>{label}</Label>
        <div className='space-y-3'>
          <div>
            <TooltipProvider>
              <Tooltip>
                <TooltipTrigger asChild>
                  <code className='cursor-help rounded bg-muted px-2 py-1 font-mono text-sm'>
                    {commit.sha.substring(0, 7)}
                  </code>
                </TooltipTrigger>
                <TooltipContent>
                  <div className='space-y-1 text-sm'>
                    <p className='font-mono'>{commit.sha}</p>
                    <div className='space-y-1 text-sm'>
                      <div className='flex items-center gap-1'>
                        <MessageSquare className='h-3 w-3' />
                        <span>{commit.message}</span>
                      </div>
                      <div className='flex items-center gap-1'>
                        <User className='h-3 w-3' />
                        <span className='truncate'>{commit.author_name}</span>
                      </div>
                      <div className='flex items-center gap-1'>
                        <Calendar className='h-3 w-3' />
                        <span>{new Date(commit.date).toLocaleString()}</span>
                      </div>
                    </div>
                  </div>
                </TooltipContent>
              </Tooltip>
            </TooltipProvider>
          </div>
        </div>
      </div>
    );
  }

  if (revision) {
    return (
      <div className='space-y-3'>
        <Label className='font-medium text-sm'>{label}</Label>
        <div className='space-y-2'>
          <TooltipProvider>
            <Tooltip>
              <TooltipTrigger asChild>
                <code className='cursor-help rounded bg-muted px-2 py-1 font-mono text-sm'>
                  {revision.substring(0, 7)}
                </code>
              </TooltipTrigger>
              <TooltipContent>
                <p className='font-mono'>{revision}</p>
              </TooltipContent>
            </Tooltip>
          </TooltipProvider>
          <p className='text-muted-foreground text-xs'>Detailed commit information unavailable</p>
        </div>
      </div>
    );
  }

  return (
    <div className='space-y-3'>
      <Label className='font-medium text-sm'>{label}</Label>
      <div>
        <code className='rounded bg-muted px-2 py-1 font-mono text-sm'>None</code>
      </div>
    </div>
  );
};
