import { Loader2 } from "lucide-react";
import { TooltipContent } from "@/components/ui/shadcn/tooltip";
import {
  TooltipProvider,
  TooltipTrigger,
} from "@/components/ui/shadcn/tooltip";
import { Tooltip } from "@/components/ui/shadcn/tooltip";
import { FileState } from "@/components/FileEditor";

interface FileStatusProps {
  fileState: FileState;
}

const FileStatus = ({ fileState }: FileStatusProps) => {
  return (
    <>
      {fileState === "saving" && (
        <Loader2 className="w-4 h-4 text-yellow-500 animate-[spin_0.2s_linear_infinite]" />
      )}
      {fileState === "modified" && (
        <TooltipProvider>
          <Tooltip>
            <TooltipTrigger>
              <div className="h-1.5 w-1.5 rounded-full bg-warning"></div>
            </TooltipTrigger>
            <TooltipContent>
              <p>Unsaved changes</p>
            </TooltipContent>
          </Tooltip>
        </TooltipProvider>
      )}
    </>
  );
};

export default FileStatus;
