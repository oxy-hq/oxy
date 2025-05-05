import { FileState } from "@/components/FileEditor";
import { Button } from "@/components/ui/shadcn/button";
import { Loader2 } from "lucide-react";

interface PageHeaderProps {
  onSave: () => void;
  filePath: string;
  fileState: FileState;
}

const PageHeader = ({ onSave, filePath, fileState }: PageHeaderProps) => {
  return (
    <div className="h-12 flex items-center justify-between p-4 bg-sidebar-background">
      <div></div>
      <p className="text-sm">{filePath}</p>
      <div className="flex items-center">
        {fileState == "modified" && (
          <Button variant="secondary" size="sm" onClick={onSave}>
            Save changes
          </Button>
        )}
        {fileState == "saving" && (
          <Loader2 className="w-4 h-4 text-yellow-500 animate-[spin_0.2s_linear_infinite]" />
        )}
      </div>
    </div>
  );
};

export default PageHeader;
