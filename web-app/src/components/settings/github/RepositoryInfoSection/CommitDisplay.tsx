import { Label } from "@/components/ui/shadcn/label";
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from "@/components/ui/shadcn/tooltip";
import { MessageSquare, User, Calendar } from "lucide-react";
import { CommitInfo } from "@/types/settings";

interface CommitDisplayProps {
  commit?: CommitInfo;
  revision?: string;
  label: string;
}

export const CommitDisplay = ({
  commit,
  revision,
  label,
}: CommitDisplayProps) => {
  // If we have detailed commit info, use it
  if (commit) {
    return (
      <div className="space-y-2">
        <Label className="text-sm font-medium">{label}</Label>
        <div className="space-y-2">
          <div className="flex items-center gap-2">
            <TooltipProvider>
              <Tooltip>
                <TooltipTrigger asChild>
                  <code className="px-2 py-1 bg-muted rounded text-sm font-mono cursor-help">
                    {commit.sha.substring(0, 7)}
                  </code>
                </TooltipTrigger>
                <TooltipContent>
                  <p className="font-mono">{commit.sha}</p>
                </TooltipContent>
              </Tooltip>
            </TooltipProvider>
          </div>
          <div className="flex items-start gap-2 text-sm">
            <MessageSquare className="h-4 w-4 mt-0.5 text-muted-foreground flex-shrink-0" />
            <span className="text-muted-foreground break-words">
              {commit.message}
            </span>
          </div>
          <div className="flex flex-col sm:flex-row sm:items-center gap-2 sm:gap-4 text-sm text-muted-foreground">
            <div className="flex items-center gap-1">
              <User className="h-3 w-3" />
              <span className="truncate">{commit.author_name}</span>
            </div>
            <div className="flex items-center gap-1">
              <Calendar className="h-3 w-3" />
              <span>{new Date(commit.date).toLocaleString()}</span>
            </div>
          </div>
        </div>
      </div>
    );
  }

  // Fallback to basic revision display
  if (revision) {
    return (
      <div className="space-y-2">
        <Label className="text-sm font-medium">{label}</Label>
        <div className="flex items-center gap-2">
          <TooltipProvider>
            <Tooltip>
              <TooltipTrigger asChild>
                <code className="px-2 py-1 bg-muted rounded text-sm font-mono cursor-help">
                  {revision.substring(0, 7)}
                </code>
              </TooltipTrigger>
              <TooltipContent>
                <p className="font-mono">{revision}</p>
              </TooltipContent>
            </Tooltip>
          </TooltipProvider>
        </div>
        <p className="text-xs text-muted-foreground">
          Detailed commit information unavailable
        </p>
      </div>
    );
  }

  // No information available
  return (
    <div className="space-y-2">
      <Label className="text-sm font-medium">{label}</Label>
      <div className="flex items-center gap-2">
        <code className="px-2 py-1 bg-muted rounded text-sm font-mono">
          None
        </code>
      </div>
    </div>
  );
};
