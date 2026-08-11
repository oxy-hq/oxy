/**
 * The "why this range" panel above the backfill date inputs.
 *
 * Presentation only — the derivation lives in `backfillSuggestion.ts`. Its job
 * is to make a pre-filled window explicable and, just as importantly, to keep
 * a *single* suggested window from reading as the right answer for resources
 * that disagree or that declared nothing at all. Every resource the suggestion
 * does not speak for is named, with the reason.
 */

import { ChevronDown, Info, Wand2 } from "lucide-react";
import type React from "react";
import { useState } from "react";

import { Button } from "@/components/ui/shadcn/button";
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger
} from "@/components/ui/shadcn/collapsible";
import { cn } from "@/libs/shadcn/utils";

import {
  type BackfillSuggestion,
  EXCLUSION_LABEL,
  suggestionRationale
} from "./backfillSuggestion";
import { formatDurationMs } from "./contractDisplay";

const Shell: React.FC<{ children: React.ReactNode }> = ({ children }) => (
  <div className='flex flex-col gap-1.5 rounded-md border border-border bg-muted/40 px-3 py-2 text-xs'>
    {children}
  </div>
);

/** Per-resource disclosure — the guard against "one window fits all". */
const ResourceBreakdown: React.FC<{ suggestion: BackfillSuggestion }> = ({ suggestion }) => {
  const [open, setOpen] = useState(false);
  const { declared, excluded } = suggestion;
  const total = declared.length + excluded.length;
  if (total === 0) return null;

  return (
    <Collapsible open={open} onOpenChange={setOpen}>
      <CollapsibleTrigger className='flex items-center gap-1 text-muted-foreground hover:text-foreground'>
        <ChevronDown className={cn("h-3 w-3 transition-transform", !open && "-rotate-90")} />
        {total} resource{total === 1 ? "" : "s"} · {declared.length} with a declared window
      </CollapsibleTrigger>
      <CollapsibleContent>
        <dl className='mt-1.5 grid grid-cols-[minmax(0,1fr)_auto] gap-x-3 gap-y-1'>
          {declared.map((d) => (
            <div key={d.resource} className='contents'>
              <dt className='truncate font-medium text-foreground'>{d.resource}</dt>
              <dd className='text-right text-primary tabular-nums'>
                {d.floorOnly ? "≥ " : ""}
                {formatDurationMs(d.windowMs)}
              </dd>
            </div>
          ))}
          {excluded.map((e) => (
            <div key={e.resource} className='contents'>
              <dt className='truncate text-muted-foreground'>{e.resource}</dt>
              <dd className='text-right text-muted-foreground'>{EXCLUSION_LABEL[e.reason]}</dd>
            </div>
          ))}
        </dl>
      </CollapsibleContent>
    </Collapsible>
  );
};

type Props = {
  suggestion: BackfillSuggestion;
  /** Contracts are still arriving — don't claim "nothing declared" yet. The
   *  hook guarantees this clears when the run's stream ends, so this branch is
   *  a wait and never a resting state. */
  loading: boolean;
  /** The pipeline has never run, so nothing has ever reported a contract. */
  neverRan: boolean;
  /** The run history could not be read — so whether a contract exists is
   *  unknown, which is not the same as there being none. */
  runsError: boolean;
  /** The form currently holds exactly the suggested range. */
  applied: boolean;
  /** Put the suggested range back after the operator edited it. */
  onApply: () => void;
};

const BackfillWindowHint: React.FC<Props> = ({
  suggestion,
  loading,
  neverRan,
  runsError,
  applied,
  onApply
}) => {
  if (loading) {
    return (
      <Shell>
        <span className='text-muted-foreground'>Reading the source contracts…</span>
      </Shell>
    );
  }

  // A failed read is not evidence of absence. Said before `neverRan`, which
  // would otherwise claim "this pipeline has not run yet" about a request that
  // simply did not complete.
  if (runsError) {
    return (
      <Shell>
        <span className='flex items-start gap-1.5 text-muted-foreground'>
          <Info className='mt-0.5 h-3 w-3 shrink-0' />
          This pipeline&apos;s run history could not be read, so no source contract could be
          consulted — not that none exists. Choose a range yourself, or retry.
        </span>
      </Shell>
    );
  }

  if (neverRan) {
    return (
      <Shell>
        <span className='flex items-start gap-1.5 text-muted-foreground'>
          <Info className='mt-0.5 h-3 w-3 shrink-0' />
          This pipeline has not run yet, so no source contract has been reported. Choose a range
          yourself.
        </span>
      </Shell>
    );
  }

  const rationale = suggestionRationale(suggestion);
  const w = suggestion.window;

  // The latest run named no resources at all — it ended before planning
  // (admission refusal, a connector build error), so nothing ever reported a
  // contract. Distinct from "resources declared nothing": there is no roster
  // to break down, and the reason to look at is the run, not the source.
  if (suggestion.declared.length === 0 && suggestion.excluded.length === 0) {
    return (
      <Shell>
        <span className='flex items-start gap-1.5 text-muted-foreground'>
          <Info className='mt-0.5 h-3 w-3 shrink-0' />
          The latest run reported no source contracts — it ended before it planned any resources, so
          there is no contract data for this pipeline to read a window from. Choose a range
          yourself.
        </span>
      </Shell>
    );
  }

  // No window anywhere. Say so plainly and suggest nothing — an absent
  // contract is *unknown*, and a default range here would be a guess wearing
  // the authority of a vendor fact.
  if (!w || !rationale) {
    return (
      <Shell>
        <span className='flex items-start gap-1.5 text-muted-foreground'>
          <Info className='mt-0.5 h-3 w-3 shrink-0' />
          No resource declares a restatement window, so there is no window to suggest. How far back
          this source restates is unknown, not zero — choose a range yourself.
        </span>
        <ResourceBreakdown suggestion={suggestion} />
      </Shell>
    );
  }

  return (
    <Shell>
      <div className='flex items-start justify-between gap-2'>
        <span className='text-foreground'>
          Suggested from the source contract:{" "}
          <span className='font-medium tabular-nums'>
            {w.floorOnly ? "at least " : "last "}
            {formatDurationMs(w.windowMs)}
          </span>{" "}
          <span className='text-muted-foreground'>({rationale})</span>
        </span>
        {!applied && (
          <Button
            type='button'
            size='sm'
            variant='ghost'
            className='h-6 shrink-0 px-2 text-xs'
            onClick={onApply}
            aria-label='Use the suggested backfill window'
          >
            <Wand2 className='h-3 w-3' />
            Use suggested
          </Button>
        )}
      </div>

      {suggestion.disagree && (
        <span className='text-muted-foreground'>
          Resources declare different windows. This range covers the widest; the narrower ones are
          simply re-read further back than they need.
        </span>
      )}
      {w.floorOnly && (
        <span className='text-muted-foreground'>
          That resource is re-pulled by partition, so its declared width is a floor — this range may
          not be far enough.
        </span>
      )}
      {suggestion.excluded.length > 0 && (
        <span className='text-muted-foreground'>
          The suggestion does not speak for {suggestion.excluded.length} resource
          {suggestion.excluded.length === 1 ? "" : "s"}.
        </span>
      )}

      <ResourceBreakdown suggestion={suggestion} />
    </Shell>
  );
};

export default BackfillWindowHint;
