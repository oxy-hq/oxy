import { FileState } from ".";
import { Check, Loader2 } from "lucide-react";
import { AlertCircle } from "lucide-react";

interface Props {
  fileState: FileState;
}

const FileStatus = ({ fileState }: Props) => {
  switch (fileState) {
    case "saved":
      return (
        <>
          <Check className="w-4 h-4 text-green-500" />
          <span className="text-sm text-muted-foreground">
            All changes saved
          </span>
        </>
      );
    case "modified":
      return (
        <>
          <AlertCircle className="w-4 h-4 text-yellow-500" />
          <span className="text-sm text-muted-foreground">Unsaved changes</span>
        </>
      );
    case "saving":
      return (
        <>
          <Loader2 className="w-4 h-4 text-yellow-500 animate-[spin_0.2s_linear_infinite]" />
          <span className="text-sm text-muted-foreground">Saving...</span>
        </>
      );
  }
};

export default FileStatus;
