import React from "react";
import { Checkbox } from "@/components/ui/checkbox";
import EmptyState from "@/components/ui/EmptyState";
import { Label } from "@/components/ui/shadcn/label";
import type { LogItem } from "@/services/types";
import Header from "./Header";
import OutputLogs from "./Logs";
import RunSelection from "./RunSelection";

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
  onArtifactClick
}) => {
  const [showLogs, setShowLogs] = React.useState(true);
  const [expandAll, setExpandAll] = React.useState(0);
  const [collapseAll, setCollapseAll] = React.useState(0);

  const handleExpandAll = () => {
    setExpandAll((prev) => prev + 1);
  };

  const handleCollapseAll = () => {
    setCollapseAll((prev) => prev + 1);
  };

  return (
    <div className='flex h-full flex-col bg-card'>
      <Header
        toggleOutput={toggleOutput}
        logs={logs}
        onExpandAll={handleExpandAll}
        onCollapseAll={handleCollapseAll}
      />

      <div className='flex items-center justify-between bg-card p-4'>
        <RunSelection workflowId={workflowId} runId={runId} />
        <div className='flex items-center gap-2'>
          <Checkbox
            name='show_logs'
            checked={showLogs}
            onCheckedChange={() => setShowLogs(!showLogs)}
          />
          <Label htmlFor='show_logs'>Show logs</Label>
        </div>
      </div>
      {logs.length === 0 && (
        <EmptyState
          className='mt-[150px] [&>img]:opacity-100'
          title='No logs yet'
          description='Run the automation to see the logs'
        />
      )}

      {logs.length > 0 && (
        <div className='min-h-0 flex-1'>
          <OutputLogs
            onArtifactClick={onArtifactClick}
            isPending={isPending}
            logs={logs}
            onlyShowResult={!showLogs}
            expandAll={expandAll}
            collapseAll={collapseAll}
          />
        </div>
      )}
    </div>
  );
};

export default WorkflowOutput;
