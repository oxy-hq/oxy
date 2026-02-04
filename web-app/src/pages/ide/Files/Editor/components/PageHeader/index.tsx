import { Loader2 } from "lucide-react";
import type { FileState } from "@/components/FileEditor";
import { Button } from "@/components/ui/shadcn/button";

interface PageHeaderProps {
  onSave: () => void;
  filePath: string;
  fileState: FileState;
}

const PageHeader = ({ onSave, filePath, fileState }: PageHeaderProps) => {
  return (
    <div className='flex h-12 items-center justify-between bg-sidebar-background p-4'>
      <div></div>
      <p className='text-sm' data-testid='ide-breadcrumb'>
        {filePath}
      </p>
      <div className='flex items-center'>
        {fileState === "modified" && (
          <Button variant='secondary' size='sm' onClick={onSave} data-testid='ide-save-button'>
            Save changes
          </Button>
        )}
        {fileState === "saving" && (
          <Loader2 className='h-4 w-4 animate-[spin_0.2s_linear_infinite] text-yellow-500' />
        )}
      </div>
    </div>
  );
};

export default PageHeader;
