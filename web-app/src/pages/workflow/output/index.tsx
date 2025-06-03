import React from "react";
import { LogItem } from "@/hooks/api/runWorkflow";
import Header from "./Header";
import OutputLogs from "./Logs";
import EmptyState from "@/components/ui/EmptyState";

interface WorkflowOutputProps {
  showOutput: boolean;
  toggleOutput: () => void;
  isPending: boolean;
  logs: LogItem[];
}

const WorkflowOutput: React.FC<WorkflowOutputProps> = ({
  showOutput,
  toggleOutput,
  isPending,
  logs,
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
          <OutputLogs isPending={isPending} logs={logs} />
        </div>
      )}
    </div>
  );
};

export default WorkflowOutput;
