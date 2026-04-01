import { Check, Copy, Maximize2, Minimize2 } from "lucide-react";
import React from "react";
import { Checkbox } from "@/components/ui/checkbox";
import EmptyState from "@/components/ui/EmptyState";
import { Panel, PanelContent, PanelHeader } from "@/components/ui/panel";
import { Button } from "@/components/ui/shadcn/button";
import { Label } from "@/components/ui/shadcn/label";
import type { LogItem } from "@/services/types";
import OutputLogs from "./Logs";
import RunSelection from "./RunSelection";
import { useCopyTimeout } from "./useCopyTimeout";

const getAllContent = (items: LogItem[]): string => {
  let content = "";
  items.forEach((item) => {
    if (!item.children || item.children.length === 0) {
      content += `${item.content}\n\n`;
    } else {
      content += getAllContent(item.children);
    }
  });
  return content.trim();
};

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
  const [allExpanded, setAllExpanded] = React.useState(false);
  const [expandAll, setExpandAll] = React.useState(0);
  const [collapseAll, setCollapseAll] = React.useState(0);
  const { copied, handleCopy } = useCopyTimeout();

  const handleToggleAll = () => {
    if (allExpanded) {
      setCollapseAll((prev) => prev + 1);
    } else {
      setExpandAll((prev) => prev + 1);
    }
    setAllExpanded((prev) => !prev);
  };

  const handleCopyAll = async () => handleCopy(getAllContent(logs));

  const actions =
    logs.length > 0 ? (
      <>
        <Button
          variant='ghost'
          size='icon'
          className='h-7 w-7'
          onClick={handleToggleAll}
          title={allExpanded ? "Collapse all" : "Expand all"}
          aria-label={allExpanded ? "Collapse all" : "Expand all"}
        >
          {allExpanded ? <Minimize2 className='h-4 w-4' /> : <Maximize2 className='h-4 w-4' />}
        </Button>
        <Button
          variant='ghost'
          size='icon'
          className='h-7 w-7'
          onClick={handleCopyAll}
          title='Copy all outputs'
          aria-label='Copy all outputs'
        >
          {copied ? <Check className='h-4 w-4 text-green-500' /> : <Copy className='h-4 w-4' />}
        </Button>
      </>
    ) : undefined;

  return (
    <Panel>
      <PanelHeader title='Output' actions={actions} onClose={toggleOutput} />
      <div className='flex shrink-0 items-center justify-between border-b px-4 py-2'>
        <RunSelection workflowId={workflowId} runId={runId} />
        <div className='flex items-center gap-2'>
          <Checkbox
            id='show_workflow_logs'
            checked={showLogs}
            onCheckedChange={() => setShowLogs(!showLogs)}
          />
          <Label htmlFor='show_workflow_logs'>Show logs</Label>
        </div>
      </div>

      {logs.length === 0 ? (
        <PanelContent>
          <EmptyState
            className='mt-[150px] [&>img]:opacity-100'
            title='No logs yet'
            description='Run the procedure to see the logs'
          />
        </PanelContent>
      ) : (
        <PanelContent scrollable={false} padding={false}>
          <OutputLogs
            onArtifactClick={onArtifactClick}
            isPending={isPending}
            logs={logs}
            onlyShowResult={!showLogs}
            expandAll={expandAll}
            collapseAll={collapseAll}
          />
        </PanelContent>
      )}
    </Panel>
  );
};

export default WorkflowOutput;
