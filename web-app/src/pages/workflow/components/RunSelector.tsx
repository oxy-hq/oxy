import type React from "react";

import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue
} from "@/components/ui/shadcn/select";
import { useWorkflowRunsForWorkflow } from "@/hooks/api/agentic-workflows/useAgenticWorkflows";

interface Props {
  /** Workflow ref the dropdown lists runs for (decoded path, e.g. `workflows/foo.workflow.yml`). */
  workflowRef: string;
  /** Currently-selected run id, if any. */
  runId?: string;
  onSelect: (runId: string) => void;
}

const formatTime = (iso: string): string => {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  return d.toLocaleString(undefined, {
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit"
  });
};

const statusDot = (status: string): string => {
  // Backend (`RunStatus` in agentic-runtime) serializes to "done" /
  // "running" / "failed" / "cancelled". The completed/succeeded/
  // errored/starting aliases are kept for forward-compat in case
  // upstream ever broadens the wire enum.
  switch (status) {
    case "done":
    case "completed":
    case "succeeded":
      return "bg-success";
    case "running":
    case "starting":
      return "bg-primary animate-pulse";
    case "failed":
    case "errored":
      return "bg-destructive";
    case "cancelled":
      return "bg-muted-foreground";
    default:
      return "bg-muted-foreground";
  }
};

const RunSelector: React.FC<Props> = ({ workflowRef, runId, onSelect }) => {
  const { data: runs, isPending } = useWorkflowRunsForWorkflow(workflowRef);

  return (
    <Select value={runId ?? ""} onValueChange={onSelect}>
      <SelectTrigger className='h-8 w-64'>
        <SelectValue placeholder='Select a run' />
      </SelectTrigger>
      <SelectContent>
        {isPending && <div className='p-3 text-muted-foreground text-xs'>Loading…</div>}
        {!isPending && (!runs || runs.length === 0) && (
          <div className='p-3 text-muted-foreground text-xs'>No runs yet</div>
        )}
        {runs?.map((run) => (
          <SelectItem key={run.run_id} value={run.run_id} className='cursor-pointer'>
            <div className='flex w-full items-center gap-2'>
              <span className={`h-2 w-2 rounded-full ${statusDot(run.status)}`} />
              <span className='font-medium font-mono text-xs'>{run.run_id.slice(0, 8)}</span>
              <span className='text-muted-foreground text-xs'>
                {formatTime(run.updated_at ?? run.created_at)}
              </span>
            </div>
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  );
};

export default RunSelector;
