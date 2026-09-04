/**
 * Manual date-window backfill dialog for an airway pipeline.
 *
 * Pins a fixed `[start, end]` range onto the date-windowed sources (Toast,
 * QuickBooks, Amazon SP-API) and seeds a normal run via
 * `POST /agentic-airway/backfill`. The pipeline's live incremental cursor is
 * never disturbed by a replay — QuickBooks freezes its cursor outright, while
 * Toast and SP-API advance one in a run-scoped store instead, so an interrupted
 * backfill can resume without the live position ever moving. Non-date-windowed
 * source kinds are rejected by the backend.
 *
 * The UI treats the end date as **inclusive**; the backend window is half-open
 * `[from, to)`, so the exclusive upper bound is pushed to the next UTC midnight.
 *
 * The range is **pre-filled from the resources' source contracts** when they
 * declare a restatement window — see `backfillSuggestion.ts` for the
 * derivation and `BackfillWindowHint` for how it is explained. It is a
 * suggestion, not a cage: the operator's first edit takes the fields over for
 * the rest of the session, and "Use suggested" puts it back.
 */

import { CalendarClock, Layers, Loader2 } from "lucide-react";
import type React from "react";
import { useEffect, useState } from "react";
import { toast } from "sonner";

import BackfillWindowHint from "@/components/airway/BackfillWindowHint";
import { Button } from "@/components/ui/shadcn/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger
} from "@/components/ui/shadcn/dialog";
import { Input } from "@/components/ui/shadcn/input";
import { Label } from "@/components/ui/shadcn/label";
import {
  useBackfillAirway,
  useBackfillSuggestion,
  useChunkedBackfill
} from "@/hooks/api/airway/useAirway";
import type { ChunkGranularity } from "@/services/api/airway";

/** `YYYY-MM-DD` → RFC3339 at UTC midnight (inclusive lower bound). */
const startOfDayUtc = (d: string): string => `${d}T00:00:00.000Z`;

/** End date is inclusive in the UI; the backend window is half-open
 *  `[from, to)`, so push the exclusive bound to the next UTC midnight. */
const endExclusiveUtc = (d: string): string => {
  const dt = new Date(`${d}T00:00:00.000Z`);
  dt.setUTCDate(dt.getUTCDate() + 1);
  return dt.toISOString();
};

type BackfillMode = "single" | "chunked";

const BackfillAirwayModal: React.FC<{
  pipelineRef: string;
  /** Called with the new run id once a single-window backfill run is seeded. */
  onStarted: (runId: string) => void;
  /** Called after a chunked backfill is enqueued (the server drives it
   *  detached); the parent typically switches to the Coverage tab to watch it. */
  onChunkedStarted?: () => void;
}> = ({ pipelineRef, onStarted, onChunkedStarted }) => {
  const [open, setOpen] = useState(false);
  const [mode, setMode] = useState<BackfillMode>("single");
  const [from, setFrom] = useState("");
  const [to, setTo] = useState("");
  const [granularity, setGranularity] = useState<ChunkGranularity>("month");
  // Set by the operator's first edit. Once true the contract suggestion never
  // writes the fields again — a suggestion that reasserts itself is a cage.
  const [touched, setTouched] = useState(false);
  const backfill = useBackfillAirway();
  const chunked = useChunkedBackfill();
  // Only streams while the dialog is on screen.
  const { suggestion, loading, neverRan, runsError } = useBackfillSuggestion(pipelineRef, open);
  const suggested = suggestion.window;

  const applySuggested = () => {
    if (!suggested) return;
    setFrom(suggested.fromDate);
    setTo(suggested.toDate);
  };

  // Pre-fill once the contracts arrive (they land a beat after the dialog
  // opens, over SSE), and only while the fields are still untouched.
  useEffect(() => {
    if (!open || touched || !suggested) return;
    setFrom(suggested.fromDate);
    setTo(suggested.toDate);
  }, [open, touched, suggested]);

  const openChange = (next: boolean) => {
    setOpen(next);
    // Closing clears the form so the next open re-derives from whatever the
    // contracts say then, rather than resurrecting a stale hand-typed range.
    if (!next) {
      setFrom("");
      setTo("");
      setTouched(false);
    }
  };

  // Inclusive [from, to]; valid when both are set and from <= to (lexical
  // compare is correct for ISO `YYYY-MM-DD`).
  const valid = from !== "" && to !== "" && from <= to;
  const busy = backfill.isPending || chunked.isPending;
  const applied = !!suggested && from === suggested.fromDate && to === suggested.toDate;

  const submit = async () => {
    if (!valid) return;
    const range = {
      pipeline_ref: pipelineRef,
      from: startOfDayUtc(from),
      to: endExclusiveUtc(to)
    };
    try {
      if (mode === "single") {
        const { run_id } = await backfill.mutateAsync(range);
        toast.success("Backfill started");
        openChange(false);
        onStarted(run_id);
      } else {
        // Chunks are serialized server-side (they share one staging buffer);
        // sending anything else would only produce an ignored-value warning.
        const { chunk_count } = await chunked.mutateAsync({
          ...range,
          granularity,
          concurrency: 1
        });
        toast.success(
          `Chunked backfill started — ${chunk_count} ${granularity} chunk${chunk_count === 1 ? "" : "s"}`
        );
        openChange(false);
        onChunkedStarted?.();
      }
    } catch (e) {
      toast.error(e instanceof Error ? `Backfill failed: ${e.message}` : "Backfill failed");
    }
  };

  return (
    <Dialog open={open} onOpenChange={openChange}>
      <DialogTrigger asChild>
        <Button size='sm' variant='outline' aria-label='Backfill this pipeline'>
          <CalendarClock className='h-4 w-4' />
          Backfill
        </Button>
      </DialogTrigger>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Backfill a date range</DialogTitle>
          <DialogDescription>
            Re-pull a historical window for the date-windowed sources (Toast, QuickBooks, Amazon
            SP-API). The pipeline&apos;s live incremental cursor is left untouched.
          </DialogDescription>
        </DialogHeader>

        {/* Single-shot window vs resumable, checkpointed chunks. */}
        <div className='flex gap-2 pt-1'>
          <Button
            type='button'
            size='sm'
            variant={mode === "single" ? "default" : "outline"}
            className='flex-1'
            onClick={() => setMode("single")}
          >
            <CalendarClock className='h-4 w-4' />
            Single window
          </Button>
          <Button
            type='button'
            size='sm'
            variant={mode === "chunked" ? "default" : "outline"}
            className='flex-1'
            onClick={() => setMode("chunked")}
          >
            <Layers className='h-4 w-4' />
            Chunked (resumable)
          </Button>
        </div>

        <BackfillWindowHint
          suggestion={suggestion}
          loading={loading}
          neverRan={neverRan}
          runsError={runsError}
          applied={applied}
          onApply={applySuggested}
        />

        <div className='grid grid-cols-2 gap-4 py-2'>
          <div className='flex flex-col gap-2'>
            <Label htmlFor='backfill-from'>Start date</Label>
            <Input
              id='backfill-from'
              type='date'
              value={from}
              max={to || undefined}
              onChange={(e) => {
                setTouched(true);
                setFrom(e.target.value);
              }}
            />
          </div>
          <div className='flex flex-col gap-2'>
            <Label htmlFor='backfill-to'>End date (inclusive)</Label>
            <Input
              id='backfill-to'
              type='date'
              value={to}
              min={from || undefined}
              onChange={(e) => {
                setTouched(true);
                setTo(e.target.value);
              }}
            />
          </div>
        </div>

        {mode === "chunked" && (
          <div className='flex flex-col gap-2'>
            {/* Single column since the "Parallel chunks" field was removed —
                a two-column grid would leave the selector at half width with an
                empty cell beside it. */}
            <div className='flex flex-col gap-4'>
              <div className='flex flex-col gap-2'>
                <Label htmlFor='backfill-granularity'>Chunk size</Label>
                <select
                  id='backfill-granularity'
                  value={granularity}
                  onChange={(e) => setGranularity(e.target.value as ChunkGranularity)}
                  className='h-9 w-full rounded-md border border-input bg-background px-3 py-1 text-sm ring-offset-background'
                >
                  <option value='month'>Month</option>
                  <option value='week'>Week</option>
                  <option value='day'>Day</option>
                </select>
              </div>
            </div>
            <p className='text-muted-foreground text-xs'>
              The window is split into {granularity} chunks, each checkpointed so the backfill
              resumes where it left off. Chunks run one at a time — they share a single staging
              buffer, so running them in parallel lets one chunk's merge consume another's
              half-loaded rows. Track progress in the Coverage tab.
            </p>
          </div>
        )}

        <DialogFooter>
          <Button onClick={submit} disabled={!valid || busy} aria-label='Start backfill'>
            {busy ? (
              <Loader2 className='h-4 w-4 animate-spin' />
            ) : mode === "single" ? (
              <CalendarClock className='h-4 w-4' />
            ) : (
              <Layers className='h-4 w-4' />
            )}
            {mode === "single" ? "Start backfill" : "Start chunked backfill"}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
};

export default BackfillAirwayModal;
