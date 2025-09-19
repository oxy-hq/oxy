import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from "@/components/ui/shadcn/tooltip";
import { Badge } from "@/components/ui/shadcn/badge";
import { CommitInfo } from "@/types/settings";

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
      <Badge variant="outline" className="text-muted-foreground">
        No commit
      </Badge>
    );
  }

  return (
    <div className="flex items-center gap-2">
      <TooltipProvider>
        <Tooltip>
          <TooltipTrigger asChild>
            <Badge
              variant="outline"
              className="font-mono text-xs cursor-help bg-secondary/50 hover:bg-secondary border-secondary-foreground/20 text-secondary-foreground/80 px-2 py-1"
            >
              {shortHash}
            </Badge>
          </TooltipTrigger>
          <TooltipContent side="bottom">
            <p className="font-mono text-xs">{commitHash}</p>
          </TooltipContent>
        </Tooltip>
      </TooltipProvider>

      {message && (
        <TooltipProvider>
          <Tooltip>
            <TooltipTrigger asChild>
              <span className="text-sm text-muted-foreground truncate max-w-32 cursor-help hover:text-foreground transition-colors">
                {message}
              </span>
            </TooltipTrigger>
            <TooltipContent className="max-w-sm" side="bottom">
              <p className="whitespace-normal break-words text-sm">{message}</p>
            </TooltipContent>
          </Tooltip>
        </TooltipProvider>
      )}
    </div>
  );
};
