import { WorkflowArtifact } from "@/types/artifact";
import { ReactFlowProvider } from "@xyflow/react";
import useWorkflowConfig from "@/hooks/api/workflows/useWorkflowConfig";
import { Skeleton } from "@/components/ui/shadcn/skeleton";
import WorkflowDiagram from "@/components/workflow/WorkflowDiagram";
import WorkflowOutput from "@/components/workflow/output";

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
        <WorkflowOutput
          logs={artifact.content.value.output ?? []}
          showOutput={true}
          toggleOutput={() => {}}
          isPending={artifact.is_streaming || false}
          onArtifactClick={onArtifactClick}
        />
      </div>
    </div>
  );
};

export default WorkflowArtifactPanel;
