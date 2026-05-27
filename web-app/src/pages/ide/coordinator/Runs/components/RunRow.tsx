import { ChevronRight } from "lucide-react";
import type React from "react";
import { useNavigate } from "react-router-dom";
import { Checkbox } from "@/components/ui/shadcn/checkbox";
import { TableCell, TableRow } from "@/components/ui/shadcn/table";
import { Elapsed } from "../../components/Elapsed";
import { JobTypeBadge } from "../../components/JobTypeBadge";
import type { NormalizedRun } from "../../components/runModel";
import { StatusBadge } from "../../components/StatusBadge";
import { SystemBadge } from "../../components/SystemBadge";
import { TriggerBadge } from "../../components/TriggerBadge";
import { useCoordinatorRoutes } from "../../components/useCoordinatorRoutes";
import { formatTimestamp, formatTokens, formatUsd, shortId } from "../../components/utils";

/** One execution in the run log — whole row navigates to the run detail. */
export const RunRow: React.FC<{
  run: NormalizedRun;
  selectable: boolean;
  selected: boolean;
  onToggle: (runId: string) => void;
}> = ({ run, selectable, selected, onToggle }) => {
  const navigate = useNavigate();
  const routes = useCoordinatorRoutes();

  return (
    <TableRow
      data-testid='coordinator-run-row'
      data-run-id={run.runId}
      className='cursor-pointer'
      onClick={() => navigate(routes.RUN_DETAIL(run.runId))}
    >
      <TableCell onClick={(e) => e.stopPropagation()} className='w-8'>
        <Checkbox
          checked={selected}
          disabled={!selectable}
          onCheckedChange={() => onToggle(run.runId)}
          aria-label='Select run'
        />
      </TableCell>
      <TableCell data-label='Status'>
        <StatusBadge status={run.status} />
      </TableCell>
      <TableCell data-label='Run'>
        <div className='flex items-center gap-2'>
          {run.isSystem ? (
            <SystemBadge variant='icon' />
          ) : (
            <JobTypeBadge type={run.jobType} variant='icon' />
          )}
          <div className='min-w-0'>
            <p className='truncate text-sm'>{run.title}</p>
            <div className='flex items-center gap-2'>
              <span className='font-mono text-muted-foreground text-xs'>{shortId(run.runId)}</span>
              {run.attempt > 0 && (
                <span className='text-warning text-xs'>attempt {run.attempt + 1}</span>
              )}
              {run.tokensTotal !== null && run.tokensTotal > 0 && (
                <span
                  className='text-muted-foreground text-xs tabular-nums'
                  title='Total tokens (input + output + cache) across all LLM calls on this run'
                >
                  {formatTokens(run.tokensTotal)} tok
                </span>
              )}
              {run.costUsd !== null && (
                <span
                  className='text-muted-foreground text-xs tabular-nums'
                  title='Estimated LLM cost — derived from token counts × per-million pricing'
                >
                  ~{formatUsd(run.costUsd)}
                </span>
              )}
            </div>
          </div>
        </div>
      </TableCell>
      <TableCell data-label='Started' className='text-muted-foreground text-xs'>
        {formatTimestamp(run.startedAt)}
      </TableCell>
      <TableCell data-label='Duration' className='text-muted-foreground text-xs tabular-nums'>
        <Elapsed startIso={run.startedAt} endIso={run.endedAt ?? undefined} live={run.live} />
      </TableCell>
      <TableCell data-label='Trigger'>
        <TriggerBadge trigger={run.trigger} />
      </TableCell>
      <TableCell className='w-8'>
        <ChevronRight className='h-4 w-4 text-muted-foreground' />
      </TableCell>
    </TableRow>
  );
};
