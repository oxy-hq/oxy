import type { FileState } from "@/components/FileEditor";
import { Button } from "@/components/ui/shadcn/button";
import { Spinner } from "@/components/ui/shadcn/spinner";

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
        {fileState === "saving" && <Spinner className='text-warning' />}
      </div>
    </div>
  );
};

export default PageHeader;
