import { AlertCircle, Check, Loader2 } from "lucide-react";
import type { FileState } from ".";

interface Props {
  fileState: FileState;
}

const FileStatus = ({ fileState }: Props) => {
  switch (fileState) {
    case "saved":
      return (
        <>
          <Check className='h-4 w-4 text-green-500' />
          <span className='text-muted-foreground text-sm'>All changes saved</span>
        </>
      );
    case "modified":
      return (
        <>
          <AlertCircle className='h-4 w-4 text-yellow-500' />
          <span className='text-muted-foreground text-sm'>Unsaved changes</span>
        </>
      );
    case "saving":
      return (
        <>
          <Loader2 className='h-4 w-4 animate-[spin_0.2s_linear_infinite] text-yellow-500' />
          <span className='text-muted-foreground text-sm'>Saving...</span>
        </>
      );
  }
};

export default FileStatus;
