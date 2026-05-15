/**
 * Step list for a workflow run — vertical layout with status indicators
 * and a small grey dot on cache-hit rows.
 *
 * Intentionally minimal: this is the v1 of the new agentic-workflows page
 * and replaces the React-Flow diagram for now. The diagram can come back
 * later, but most users only care about per-step status + the live log.
 */

import { CheckCircle2, CircleDashed, Loader2, XCircle } from "lucide-react";

import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger
} from "@/components/ui/shadcn/tooltip";
import type { RunStepState } from "@/hooks/api/agentic-workflows/useAgenticWorkflows";
import { cn } from "@/libs/shadcn/utils";

type Props = {
  steps: RunStepState[];
};

export const StepList = ({ steps }: Props) => {
  if (steps.length === 0) {
    return (
      <div className='flex h-full items-center justify-center text-muted-foreground'>
        Waiting for the run to start…
      </div>
    );
  }
  return (
    <TooltipProvider delayDuration={200}>
      <ol className='flex flex-col gap-2 p-4'>
        {steps.map((step, idx) => (
          <StepRow key={step.name} step={step} index={idx} />
        ))}
      </ol>
    </TooltipProvider>
  );
};

const StepRow = ({ step, index }: { step: RunStepState; index: number }) => {
  return (
    <li
      className={cn(
        "flex items-center gap-3 rounded-md border bg-card px-3 py-2",
        step.status === "running" && "border-primary",
        step.status === "failed" && "border-destructive",
        step.status === "cached" && "border-muted-foreground/30 bg-muted/30",
        // Skipped rows fade out: the step never ran, so the row should
        // recede visually compared to active/completed steps.
        step.status === "skipped" && "opacity-50"
      )}
    >
      <span className='font-mono text-muted-foreground text-xs tabular-nums'>
        {String(index + 1).padStart(2, "0")}
      </span>
      <StatusIcon status={step.status} />
      <span className='flex-1 truncate font-medium text-sm'>{step.name}</span>
      <span className='text-muted-foreground text-xs'>{step.taskType}</span>
      {step.status === "cached" && (
        <Tooltip>
          <TooltipTrigger asChild>
            {/* The grey "reused" dot — single visual cue for cache hits.
                Tooltip provides the accessible label; the dot itself is decorative. */}
            <span
              role='img'
              aria-label='Reused from a prior run'
              className='inline-block size-2 rounded-full bg-muted-foreground/60'
            />
          </TooltipTrigger>
          <TooltipContent side='left'>
            Reused from run{step.priorRunId ? ` ${shortenId(step.priorRunId)}` : ""}
          </TooltipContent>
        </Tooltip>
      )}
      {step.status === "failed" && step.errorMessage && (
        <Tooltip>
          <TooltipTrigger asChild>
            <span className='text-destructive text-xs'>error</span>
          </TooltipTrigger>
          <TooltipContent side='left'>{step.errorMessage}</TooltipContent>
        </Tooltip>
      )}
    </li>
  );
};

const StatusIcon = ({ status }: { status: RunStepState["status"] }) => {
  switch (status) {
    case "pending":
    case "skipped":
      // Same dashed-circle glyph for both — distinguished by row-level
      // opacity (skipped fades out) and the heading label.
      return <CircleDashed className='size-4 text-muted-foreground' />;
    case "running":
      return <Loader2 className='size-4 animate-spin text-primary' />;
    case "success":
    case "cached":
      return <CheckCircle2 className='size-4 text-primary' />;
    case "failed":
      return <XCircle className='size-4 text-destructive' />;
  }
};

function shortenId(id: string): string {
  return id.length <= 8 ? id : `${id.slice(0, 8)}…`;
}
