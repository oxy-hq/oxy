/**
 * Manual date-window backfill dialog for an airway pipeline.
 *
 * Pins a fixed `[start, end]` range onto the date-windowed sources (Toast,
 * QuickBooks) and seeds a normal run via `POST /agentic-airway/backfill`. The
 * pipeline's live incremental cursor is frozen during a backfill, so a replay
 * never disturbs ongoing scheduled loads. Non-date-windowed source kinds are
 * rejected by the backend.
 *
 * The UI treats the end date as **inclusive**; the backend window is half-open
 * `[from, to)`, so the exclusive upper bound is pushed to the next UTC midnight.
 */

import { CalendarClock, Layers, Loader2 } from "lucide-react";
import type React from "react";
import { useState } from "react";
import { toast } from "sonner";

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
import { useBackfillAirway, useChunkedBackfill } from "@/hooks/api/airway/useAirway";
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
  const [concurrency, setConcurrency] = useState(4);
  const backfill = useBackfillAirway();
  const chunked = useChunkedBackfill();

  // Inclusive [from, to]; valid when both are set and from <= to (lexical
  // compare is correct for ISO `YYYY-MM-DD`).
  const valid = from !== "" && to !== "" && from <= to;
  const busy = backfill.isPending || chunked.isPending;

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
        setOpen(false);
        onStarted(run_id);
      } else {
        const { chunk_count } = await chunked.mutateAsync({ ...range, granularity, concurrency });
        toast.success(
          `Chunked backfill started — ${chunk_count} ${granularity} chunk${chunk_count === 1 ? "" : "s"}`
        );
        setOpen(false);
        onChunkedStarted?.();
      }
    } catch (e) {
      toast.error(e instanceof Error ? `Backfill failed: ${e.message}` : "Backfill failed");
    }
  };

  return (
    <Dialog open={open} onOpenChange={setOpen}>
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
            Re-pull a historical window for the date-windowed sources (Toast, QuickBooks). The
            pipeline&apos;s live incremental cursor is left untouched.
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

        <div className='grid grid-cols-2 gap-4 py-2'>
          <div className='flex flex-col gap-2'>
            <Label htmlFor='backfill-from'>Start date</Label>
            <Input
              id='backfill-from'
              type='date'
              value={from}
              max={to || undefined}
              onChange={(e) => setFrom(e.target.value)}
            />
          </div>
          <div className='flex flex-col gap-2'>
            <Label htmlFor='backfill-to'>End date (inclusive)</Label>
            <Input
              id='backfill-to'
              type='date'
              value={to}
              min={from || undefined}
              onChange={(e) => setTo(e.target.value)}
            />
          </div>
        </div>

        {mode === "chunked" && (
          <div className='flex flex-col gap-2'>
            <div className='grid grid-cols-2 gap-4'>
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
              <div className='flex flex-col gap-2'>
                <Label htmlFor='backfill-concurrency'>Parallel chunks</Label>
                <Input
                  id='backfill-concurrency'
                  type='number'
                  min={1}
                  max={16}
                  value={concurrency}
                  onChange={(e) =>
                    setConcurrency(Math.min(16, Math.max(1, Number(e.target.value) || 1)))
                  }
                />
              </div>
            </div>
            <p className='text-muted-foreground text-xs'>
              The window is split into {granularity} chunks, each checkpointed so the backfill
              resumes where it left off; up to {concurrency} run at once. Track progress in the
              Coverage tab.
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
