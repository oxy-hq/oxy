import { AlertCircle, Check } from "lucide-react";
import { Spinner } from "@/components/ui/shadcn/spinner";
import type { FileState } from ".";

interface Props {
  fileState: FileState;
}

const FileStatus = ({ fileState }: Props) => {
  switch (fileState) {
    case "saved":
      return (
        <>
          <Check className='h-4 w-4 text-success' />
          <span className='text-muted-foreground text-sm'>All changes saved</span>
        </>
      );
    case "modified":
      return (
        <>
          <AlertCircle className='h-4 w-4 text-warning' />
          <span className='text-muted-foreground text-sm'>Unsaved changes</span>
        </>
      );
    case "saving":
      return <Spinner className='text-warning' />;
  }
};

export default FileStatus;
