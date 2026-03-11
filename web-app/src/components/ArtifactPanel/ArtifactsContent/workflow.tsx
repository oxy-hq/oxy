import { ReactFlowProvider } from "@xyflow/react";
import { ContentSkeleton } from "@/components/ui/ContentSkeleton";
import EmptyState from "@/components/ui/EmptyState";
import OutputLogs from "@/components/workflow/output/Logs";
import WorkflowDiagram from "@/components/workflow/WorkflowDiagram";
import useWorkflowConfig from "@/hooks/api/workflows/useWorkflowConfig";
import type { WorkflowArtifact } from "@/types/artifact";

type Props = {
  artifact: WorkflowArtifact;
  onArtifactClick?: (id: string) => void;
};

const WorkflowArtifactPanel = ({ artifact, onArtifactClick }: Props) => {
  const { data: workflowConfig } = useWorkflowConfig(artifact.content.value.ref);

  if (!workflowConfig) {
    return <ContentSkeleton />;
  }

  return (
    <div className='flex h-full flex-col'>
      <div className='flex-1'>
        <ReactFlowProvider>
          <WorkflowDiagram
            workflowId={artifact.content.value.ref}
            workflowConfig={workflowConfig}
          />
        </ReactFlowProvider>
      </div>

      <div className='flex-1 border-border border-t'>
        <div className='flex h-full flex-col bg-sidebar-background'>
          {(artifact.content.value.output ?? []).length === 0 && (
            <EmptyState
              className='mt-[150px]'
              title='No logs yet'
              description='Run the procedure to see the logs'
            />
          )}
          {(artifact.content.value.output ?? []).length > 0 && (
            <div className='min-h-0 flex-1'>
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
