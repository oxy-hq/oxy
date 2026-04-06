import type { FileState } from "@/components/FileEditor";
import { Spinner } from "@/components/ui/shadcn/spinner";
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
      {fileState === "saving" && <Spinner className='text-warning' />}
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
