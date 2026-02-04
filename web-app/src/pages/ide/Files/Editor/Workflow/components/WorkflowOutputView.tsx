import { WorkflowPreview } from "@/components/workflow/WorkflowPreview";
import ModeSwitcher from "./ModeSwitcher";
import type { WorkflowViewMode } from "./types";

interface WorkflowOutputViewProps {
  viewMode: WorkflowViewMode;
  onViewModeChange: (mode: WorkflowViewMode) => void;
  workflowPath: string;
  pathb64: string;
  runId?: string;
}

const WorkflowOutputView = ({
  viewMode,
  onViewModeChange,
  workflowPath,
  pathb64,
  runId
}: WorkflowOutputViewProps) => {
  return (
    <div className='fade-in flex h-full animate-in flex-col duration-200'>
      <div className='flex items-center gap-2 border-b px-3 py-1'>
        <ModeSwitcher viewMode={viewMode} onViewModeChange={onViewModeChange} />
        <div className='font-medium text-muted-foreground text-sm'>{workflowPath}</div>
      </div>
      <div className='flex-1 overflow-hidden'>
        <WorkflowPreview pathb64={pathb64} runId={runId} direction='horizontal' />
      </div>
    </div>
  );
};

export default WorkflowOutputView;
