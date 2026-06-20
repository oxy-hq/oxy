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

import { CalendarClock, Loader2 } from "lucide-react";
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
import { useBackfillAirway } from "@/hooks/api/airway/useAirway";

/** `YYYY-MM-DD` → RFC3339 at UTC midnight (inclusive lower bound). */
const startOfDayUtc = (d: string): string => `${d}T00:00:00.000Z`;

/** End date is inclusive in the UI; the backend window is half-open
 *  `[from, to)`, so push the exclusive bound to the next UTC midnight. */
const endExclusiveUtc = (d: string): string => {
  const dt = new Date(`${d}T00:00:00.000Z`);
  dt.setUTCDate(dt.getUTCDate() + 1);
  return dt.toISOString();
};

const BackfillAirwayModal: React.FC<{
  pipelineRef: string;
  /** Called with the new run id once the backfill run is seeded. */
  onStarted: (runId: string) => void;
}> = ({ pipelineRef, onStarted }) => {
  const [open, setOpen] = useState(false);
  const [from, setFrom] = useState("");
  const [to, setTo] = useState("");
  const backfill = useBackfillAirway();

  // Inclusive [from, to]; valid when both are set and from <= to (lexical
  // compare is correct for ISO `YYYY-MM-DD`).
  const valid = from !== "" && to !== "" && from <= to;

  const submit = async () => {
    if (!valid) return;
    try {
      const { run_id } = await backfill.mutateAsync({
        pipeline_ref: pipelineRef,
        from: startOfDayUtc(from),
        to: endExclusiveUtc(to)
      });
      toast.success("Backfill started");
      setOpen(false);
      onStarted(run_id);
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
            Re-pull a fixed historical window for the date-windowed sources (Toast, QuickBooks). The
            pipeline&apos;s live incremental cursor is left untouched.
          </DialogDescription>
        </DialogHeader>
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
        <DialogFooter>
          <Button
            onClick={submit}
            disabled={!valid || backfill.isPending}
            aria-label='Start backfill'
          >
            {backfill.isPending ? (
              <Loader2 className='h-4 w-4 animate-spin' />
            ) : (
              <CalendarClock className='h-4 w-4' />
            )}
            Start backfill
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
};

export default BackfillAirwayModal;
