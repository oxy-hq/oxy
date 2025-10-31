import React from "react";
import Header from "./Header";
import OutputLogs from "./Logs";
import EmptyState from "@/components/ui/EmptyState";
import { LogItem } from "@/services/types";
import RunSelection from "./RunSelection";
import { Checkbox } from "@/components/ui/checkbox";
import { Label } from "@/components/ui/shadcn/label";

interface WorkflowOutputProps {
  toggleOutput: () => void;
  isPending: boolean;
  logs: LogItem[];
  onArtifactClick?: (id: string) => void;
  workflowId: string;
  runId?: string;
}

const WorkflowOutput: React.FC<WorkflowOutputProps> = ({
  toggleOutput,
  isPending,
  logs,
  workflowId,
  runId,
  onArtifactClick,
}) => {
  const [showLogs, setShowLogs] = React.useState(true);
  return (
    <div className="h-full flex flex-col bg-card">
      <Header toggleOutput={toggleOutput} />

      <div className="flex justify-between items-center p-4 bg-card">
        <RunSelection workflowId={workflowId} runId={runId} />
        <div className="flex items-center gap-2">
          <Checkbox
            name="show_logs"
            checked={showLogs}
            onCheckedChange={() => setShowLogs(!showLogs)}
          />
          <Label htmlFor="show_logs">Show logs</Label>
        </div>
      </div>
      {logs.length === 0 && (
        <EmptyState
          className="mt-[150px] [&>img]:opacity-100"
          title="No logs yet"
          description="Run the workflow to see the logs"
        />
      )}

      {logs.length > 0 && (
        <div className="flex-1 min-h-0">
          <OutputLogs
            onArtifactClick={onArtifactClick}
            isPending={isPending}
            logs={logs}
            onlyShowResult={!showLogs}
          />
        </div>
      )}
    </div>
  );
};

export default WorkflowOutput;
