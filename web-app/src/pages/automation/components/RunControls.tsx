/**
 * "Run" + "Retry" buttons with an explicit popover for cache_enabled
 * and per-iteration force-replay overrides.
 *
 * Defaults: Run → fresh run, no cache. Retry → cache_enabled = true,
 * pointing at the current run id (the retry's "prior run"). The
 * popover also surfaces the prior run's per-iteration outcomes (via
 * `iterationsBySteps`) so the user can flip specific cached iterations
 * back to "force re-run" — submitted as `invalidate_iterations`.
 */

import { Loader2, PlayIcon, RotateCcw, StopCircle } from "lucide-react";
import { useEffect, useState } from "react";

import { Button } from "@/components/ui/shadcn/button";
import { Label } from "@/components/ui/shadcn/label";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/shadcn/popover";
import { Switch } from "@/components/ui/shadcn/switch";
import type { IterationOutcome } from "@/services/api/automations";

import { IterationGrid } from "./IterationGrid";

export type RetryOptions = {
  cacheEnabled: boolean;
  invalidateIterations?: Record<string, number[]>;
};

type Props = {
  /** Whether a run is currently active (for the Stop button). */
  running: boolean;
  /** Whether the user already has a finished run we could retry against. */
  hasPriorRun: boolean;
  /** Prior-run per-step iteration outcomes for the override grid. */
  iterationsBySteps?: Record<string, IterationOutcome[]>;
  starting: boolean;
  cancelling: boolean;
  onRun: () => void;
  onRetry: (opts: RetryOptions) => void;
  onStop: () => void;
};

export const RunControls = ({
  running,
  hasPriorRun,
  iterationsBySteps,
  starting,
  cancelling,
  onRun,
  onRetry,
  onStop
}: Props) => {
  const [retryOpen, setRetryOpen] = useState(false);
  const [cacheEnabled, setCacheEnabled] = useState(true);
  const [forced, setForced] = useState<Record<string, number[]>>({});

  // Clear any stale force-overrides whenever the popover closes or
  // the prior-run data set changes — avoids carrying yesterday's
  // selections into a fresh popover for a new run.
  useEffect(() => {
    if (!retryOpen) setForced({});
  }, [retryOpen]);

  if (running) {
    return (
      <Button variant='outline' size='sm' onClick={onStop} disabled={cancelling}>
        {cancelling ? (
          <Loader2 className='size-4 animate-spin' />
        ) : (
          <StopCircle className='size-4' />
        )}
        Stop
      </Button>
    );
  }

  return (
    <div className='flex items-center gap-2'>
      <Button size='sm' onClick={onRun} disabled={starting}>
        {starting ? <Loader2 className='size-4 animate-spin' /> : <PlayIcon className='size-4' />}
        Run
      </Button>

      {hasPriorRun && (
        <Popover open={retryOpen} onOpenChange={setRetryOpen}>
          <PopoverTrigger asChild>
            <Button variant='outline' size='sm'>
              <RotateCcw className='size-4' />
              Retry
            </Button>
          </PopoverTrigger>
          <PopoverContent align='end' className='w-96 space-y-3'>
            <div className='space-y-1'>
              <p className='font-medium text-sm'>Retry from prior run</p>
              <p className='text-muted-foreground text-xs'>
                Reuses results for steps whose inputs haven't changed.
              </p>
            </div>
            <div className='flex items-center justify-between'>
              <Label htmlFor='cache-toggle' className='cursor-pointer text-sm'>
                Reuse unchanged steps
              </Label>
              <Switch id='cache-toggle' checked={cacheEnabled} onCheckedChange={setCacheEnabled} />
            </div>
            {cacheEnabled && iterationsBySteps && (
              <IterationGrid steps={iterationsBySteps} forced={forced} onChange={setForced} />
            )}
            <Button
              size='sm'
              className='w-full'
              onClick={() => {
                setRetryOpen(false);
                onRetry({
                  cacheEnabled,
                  invalidateIterations: Object.keys(forced).length > 0 ? forced : undefined
                });
              }}
              disabled={starting}
            >
              {starting ? <Loader2 className='size-4 animate-spin' /> : null}
              Start retry
            </Button>
          </PopoverContent>
        </Popover>
      )}
    </div>
  );
};
