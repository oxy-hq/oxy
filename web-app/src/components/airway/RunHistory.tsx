/**
 * Past-runs list for a pipeline. Newest first; clicking a row adopts
 * that run into the live view (the controller's `setRunId`).
 */

import { Check, Loader2, X } from "lucide-react";
import type React from "react";

import { Badge } from "@/components/ui/shadcn/badge";
import { useAirwayRuns } from "@/hooks/api/airway/useAirway";
import { cn } from "@/libs/shadcn/utils";
import { timeAgo } from "@/libs/utils/date";

const STATUS_ICON: Record<string, React.ReactNode> = {
  done: <Check className='h-3.5 w-3.5' />,
  failed: <X className='h-3.5 w-3.5' />,
  cancelled: <X className='h-3.5 w-3.5' />
};

type Props = {
  pipelineRef: string;
  activeRunId?: string;
  onSelect: (runId: string) => void;
};

const RunHistory: React.FC<Props> = ({ pipelineRef, activeRunId, onSelect }) => {
  const { data: runs, isLoading, isError } = useAirwayRuns(pipelineRef);

  if (isLoading) {
    return <div className='px-4 py-3 text-muted-foreground text-xs'>Loading run history…</div>;
  }
  if (isError) {
    return <div className='px-4 py-3 text-destructive text-xs'>Couldn’t load run history.</div>;
  }
  if (!runs || runs.length === 0) {
    return (
      <div className='px-4 py-3 text-muted-foreground text-xs'>
        No past runs for this pipeline yet.
      </div>
    );
  }

  return (
    <ul className='divide-y divide-border'>
      {runs.map((r) => {
        const terminal = r.status === "done" || r.status === "failed" || r.status === "cancelled";
        return (
          <li key={r.run_id}>
            <button
              type='button'
              onClick={() => onSelect(r.run_id)}
              aria-current={r.run_id === activeRunId ? "true" : undefined}
              className={cn(
                "flex w-full items-center gap-3 px-4 py-2 text-left text-sm hover:bg-muted/50",
                r.run_id === activeRunId && "bg-muted"
              )}
            >
              <span className='text-muted-foreground'>
                {terminal ? (
                  (STATUS_ICON[r.status] ?? null)
                ) : (
                  <Loader2 className='h-3.5 w-3.5 animate-spin' />
                )}
              </span>
              <span className='font-mono text-xs'>{r.run_id.slice(0, 8)}</span>
              <Badge
                variant={
                  r.status === "done"
                    ? "default"
                    : r.status === "failed"
                      ? "destructive"
                      : "outline"
                }
              >
                {r.status}
              </Badge>
              <span className='ml-auto text-muted-foreground text-xs'>{timeAgo(r.created_at)}</span>
            </button>
          </li>
        );
      })}
    </ul>
  );
};

export default RunHistory;
