import { WorkflowArtifact } from "@/types/artifact";
import { ReactFlowProvider } from "@xyflow/react";
import useWorkflowConfig from "@/hooks/api/workflows/useWorkflowConfig";
import { Skeleton } from "@/components/ui/shadcn/skeleton";
import WorkflowDiagram from "@/components/workflow/WorkflowDiagram";
import OutputLogs from "@/components/workflow/output/Logs";
import EmptyState from "@/components/ui/EmptyState";

type Props = {
  artifact: WorkflowArtifact;
  onArtifactClick?: (id: string) => void;
};

const WorkflowArtifactPanel = ({ artifact, onArtifactClick }: Props) => {
  const { data: workflowConfig } = useWorkflowConfig(
    artifact.content.value.ref,
  );

  if (!workflowConfig) {
    return (
      <div className="flex flex-col gap-4">
        <Skeleton className="h-4 max-w-[200px]" />
        <Skeleton className="h-4 max-w-[500px]" />
        <Skeleton className="h-4 max-w-[500px]" />
      </div>
    );
  }

  return (
    <div className="flex flex-col h-full">
      <div className="flex-1">
        <ReactFlowProvider>
          <WorkflowDiagram
            workflowId={artifact.content.value.ref}
            workflowConfig={workflowConfig}
          />
        </ReactFlowProvider>
      </div>

      <div className="flex-1">
        <div className="bg-sidebar-background h-full flex flex-col">
          {(artifact.content.value.output ?? []).length === 0 && (
            <EmptyState
              className="mt-[150px]"
              title="No logs yet"
              description="Run the workflow to see the logs"
            />
          )}
          {(artifact.content.value.output ?? []).length > 0 && (
            <div className="flex-1 min-h-0">
              <OutputLogs
                onArtifactClick={onArtifactClick}
                isPending={artifact.is_streaming || false}
                logs={artifact.content.value.output ?? []}
                onlyShowResult={false}
              />
            </div>
          )}
        </div>
      </div>
    </div>
  );
};

export default WorkflowArtifactPanel;
