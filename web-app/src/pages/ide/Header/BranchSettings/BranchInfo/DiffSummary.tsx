import { File, FileDiff, FileMinus, FilePlus, Loader2, Minus, Plus } from "lucide-react";
import { useNavigate } from "react-router-dom";
import { Label } from "@/components/ui/shadcn/label";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/shadcn/tooltip";
import useDiffSummary from "@/hooks/api/files/useDiffSummary";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import type { FileStatus } from "@/types/file";

const DiffSummaryWrapper = ({ children }: { children: React.ReactNode }) => {
  return (
    <div className='max-w-full space-y-3'>
      <Label className='font-medium text-sm'>File Changes</Label>
      {children}
    </div>
  );
};

const DiffSummary = ({ onFileClick }: { onFileClick: () => void }) => {
  const { data: diffSummary, isLoading } = useDiffSummary();
  const navigate = useNavigate();
  const { project } = useCurrentProjectBranch();

  const handleFileClick = (filePath: string) => {
    if (!project) return;
    const pathb64 = btoa(filePath);
    navigate(`/projects/${project.id}/ide/${pathb64}`);
    onFileClick();
  };

  const getStatusIcon = (status: FileStatus["status"]) => {
    switch (status) {
      case "M":
        return <FileDiff className='h-4 w-4 flex-shrink-0 text-warning' />;
      case "A":
        return <FilePlus className='h-4 w-4 flex-shrink-0 text-green-600' />;
      case "D":
        return <FileMinus className='h-4 w-4 flex-shrink-0 text-destructive' />;
      default:
        return <File className='h-4 w-4 flex-shrink-0' />;
    }
  };

  if (isLoading) {
    return (
      <DiffSummaryWrapper>
        <div className='flex items-center gap-2'>
          <Loader2 className='h-4 w-4 animate-spin' />
          <span className='text-muted-foreground text-sm'>Loading changes...</span>
        </div>
      </DiffSummaryWrapper>
    );
  }

  if (!diffSummary || diffSummary.length === 0) {
    return (
      <DiffSummaryWrapper>
        <p className='text-muted-foreground text-sm'>No changes detected</p>
      </DiffSummaryWrapper>
    );
  }

  return (
    <DiffSummaryWrapper>
      {diffSummary.map((file) => (
        <Tooltip key={file.path} delayDuration={500}>
          <TooltipTrigger asChild>
            <div
              key={file.path}
              className={`flex items-center justify-between rounded-md border p-2 transition-colors ${
                file.status === "D"
                  ? "cursor-not-allowed opacity-50"
                  : "cursor-pointer hover:bg-muted/50"
              }`}
              onClick={() => file.status !== "D" && handleFileClick(file.path)}
            >
              <div className='flex min-w-0 flex-1 items-center gap-2'>
                {getStatusIcon(file.status)}
                <span className='truncate font-mono text-sm' title={file.path}>
                  {file.path}
                </span>
              </div>

              {(file.insert > 0 || file.delete > 0) && (
                <div className='flex items-center gap-2 text-muted-foreground text-xs'>
                  {file.insert > 0 && (
                    <div className='flex items-center text-green-600'>
                      <Plus className='h-3 w-3' />
                      <span>{file.insert}</span>
                    </div>
                  )}
                  {file.delete > 0 && (
                    <div className='flex items-center text-red-600'>
                      <Minus className='h-3 w-3' />
                      <span>{file.delete}</span>
                    </div>
                  )}
                </div>
              )}
            </div>
          </TooltipTrigger>
          <TooltipContent>{file.path}</TooltipContent>
        </Tooltip>
      ))}
    </DiffSummaryWrapper>
  );
};

export default DiffSummary;
