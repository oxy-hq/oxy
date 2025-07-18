import React from "react";
import Header from "./Header";
import OutputLogs from "./Logs";
import EmptyState from "@/components/ui/EmptyState";
import { LogItem } from "@/services/types";

interface WorkflowOutputProps {
  showOutput: boolean;
  toggleOutput: () => void;
  isPending: boolean;
  logs: LogItem[];
  onArtifactClick?: (id: string) => void;
}

const WorkflowOutput: React.FC<WorkflowOutputProps> = ({
  showOutput,
  toggleOutput,
  isPending,
  logs,
  onArtifactClick,
}) => {
  return (
    <div className="bg-sidebar-background h-full flex flex-col">
      <Header showOutput={showOutput} toggleOutput={toggleOutput} />
      {logs.length === 0 && (
        <EmptyState
          className="mt-[150px]"
          title="No logs yet"
          description="Run the workflow to see the logs"
        />
      )}

      {logs.length > 0 && showOutput && (
        <div className="flex-1 min-h-0">
          <OutputLogs
            onArtifactClick={onArtifactClick}
            isPending={isPending}
            logs={logs}
          />
        </div>
      )}
    </div>
  );
};

export default WorkflowOutput;
