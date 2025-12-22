import { WorkflowPreview } from "@/components/workflow/WorkflowPreview";
import ModeSwitcher from "./ModeSwitcher";
import { WorkflowViewMode } from "./types";

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
  runId,
}: WorkflowOutputViewProps) => {
  return (
    <div className="flex flex-col h-full animate-in fade-in duration-200">
      <div className="flex items-center gap-2 px-3 py-1 border-b">
        <ModeSwitcher viewMode={viewMode} onViewModeChange={onViewModeChange} />
        <div className="text-sm font-medium text-muted-foreground">
          {workflowPath}
        </div>
      </div>
      <div className="flex-1 overflow-hidden">
        <WorkflowPreview
          pathb64={pathb64}
          runId={runId}
          direction="horizontal"
        />
      </div>
    </div>
  );
};

export default WorkflowOutputView;
