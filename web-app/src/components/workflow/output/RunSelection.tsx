import React from "react";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/shadcn/select";
import { useListWorkflowRuns } from "../useWorkflowRun";
import { get } from "lodash";
import { RunInfo } from "@/services/types/runs";
import { createSearchParams, useLocation, useNavigate } from "react-router-dom";

interface Props {
  workflowId: string;
  runId?: string;
}

const RunSelection: React.FC<Props> = ({ workflowId, runId }) => {
  const location = useLocation();
  const navigate = useNavigate();

  const onRunIdChange = (newRunId: string) => {
    navigate({
      pathname: location.pathname,
      search: createSearchParams({
        run: newRunId.toString(),
      }).toString(),
    });
  };

  const { data, isPending } = useListWorkflowRuns(workflowId, {
    pageIndex: 0,
    pageSize: 10000,
  });

  const items = get(data, "items", []);

  return (
    <Select value={runId} onValueChange={onRunIdChange}>
      <SelectTrigger>
        <SelectValue placeholder="Select the run" />
      </SelectTrigger>
      <SelectContent>
        {isPending && <div className="p-4">Loading...</div>}
        {items.map((run: RunInfo) => (
          <SelectItem key={run.run_index} value={run.run_index.toString()}>
            <div className="flex space-between w-full gap-6 items-center">
              <p className="text-sm font-medium">Run {run.run_index}</p>
              <p className="text-xs text-muted-foreground">
                {new Date(run.updated_at).toLocaleString()}
              </p>
            </div>
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  );
};

export default RunSelection;
