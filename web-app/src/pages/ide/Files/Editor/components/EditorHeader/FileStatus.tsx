import { Loader2 } from "lucide-react";
import type { FileState } from "@/components/FileEditor";
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger
} from "@/components/ui/shadcn/tooltip";

interface FileStatusProps {
  fileState: FileState;
}

const FileStatus = ({ fileState }: FileStatusProps) => {
  return (
    <>
      {fileState === "saving" && (
        <Loader2 className='h-4 w-4 animate-[spin_0.2s_linear_infinite] text-yellow-500' />
      )}
      {fileState === "modified" && (
        <TooltipProvider>
          <Tooltip>
            <TooltipTrigger>
              <div className='h-1.5 w-1.5 rounded-full bg-warning'></div>
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
