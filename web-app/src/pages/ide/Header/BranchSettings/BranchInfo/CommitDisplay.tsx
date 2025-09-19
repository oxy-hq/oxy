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
  if (commit) {
    return (
      <div className="space-y-3">
        <Label className="text-sm font-medium">{label}</Label>
        <div className="space-y-3">
          <div>
            <TooltipProvider>
              <Tooltip>
                <TooltipTrigger asChild>
                  <code className="px-2 py-1 bg-muted rounded text-sm font-mono cursor-help">
                    {commit.sha.substring(0, 7)}
                  </code>
                </TooltipTrigger>
                <TooltipContent>
                  <div className="space-y-1 text-sm">
                    <p className="font-mono">{commit.sha}</p>
                    <div className="space-y-1 text-sm">
                      <div className="flex items-center gap-1">
                        <MessageSquare className="h-3 w-3" />
                        <span>{commit.message}</span>
                      </div>
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
      <div className="space-y-3">
        <Label className="text-sm font-medium">{label}</Label>
        <div className="space-y-2">
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
          <p className="text-xs text-muted-foreground">
            Detailed commit information unavailable
          </p>
        </div>
      </div>
    );
  }

  return (
    <div className="space-y-3">
      <Label className="text-sm font-medium">{label}</Label>
      <div>
        <code className="px-2 py-1 bg-muted rounded text-sm font-mono">
          None
        </code>
      </div>
    </div>
  );
};
