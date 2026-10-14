import { Loader2, RefreshCw } from "lucide-react";
import type React from "react";
import { useState } from "react";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle
} from "@/components/ui/shadcn/alert-dialog";
import { Button } from "@/components/ui/shadcn/button";

const RERUN_FAILED_TABLES_HINT = "Start a new run for the tables that failed in this run";

/** Visible text, aria-label and dialog title — one string so they can't drift. */
const rerunFailedTablesLabel = (count: number): string =>
  `Re-run ${count} failed ${count === 1 ? "table" : "tables"}`;

/**
 * "Re-run N failed tables" — an OFFER to start a new pipeline run scoped
 * to the tables that failed in this run. Worded as an action, not a
 * status: "Retry failed (N)" was read as "a retry ran and failed".
 *
 * `failedTables` must come from `airwayFailedTables.ts` (per-table
 * failure events of the latest attempt) — never from the run status, or
 * a run failed at the run level offers to re-run tables that loaded.
 *
 * Surfaces the at-least-once safety caveat: airway commits per-batch and
 * advances cursors only after a successful load, so re-running a table
 * that failed *mid-load* can duplicate the rows committed before the
 * failure when its disposition is `append`. `replace`/`merge` are
 * idempotent; extract failures committed nothing and are always safe.
 *
 * The button is presentational — the host supplies `onConfirm` (the IDE
 * page streams the new run via its controller; the coordinator starts a
 * detached run), so this component stays surface-agnostic.
 */
const RetryFailedTablesButton: React.FC<{
  failedTables: string[];
  onConfirm: (tables: string[]) => Promise<void> | void;
  pending?: boolean;
  size?: "sm" | "default";
  variant?: "default" | "outline" | "secondary";
}> = ({ failedTables, onConfirm, pending = false, size = "sm", variant = "outline" }) => {
  const [open, setOpen] = useState(false);
  if (failedTables.length === 0) return null;

  const label = rerunFailedTablesLabel(failedTables.length);
  return (
    <>
      <Button
        size={size}
        variant={variant}
        onClick={() => setOpen(true)}
        disabled={pending}
        aria-label={label}
        title={RERUN_FAILED_TABLES_HINT}
      >
        {pending ? <Loader2 className='h-4 w-4 animate-spin' /> : <RefreshCw className='h-4 w-4' />}
        {label}
      </Button>
      <AlertDialog open={open} onOpenChange={setOpen}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{label}?</AlertDialogTitle>
            <AlertDialogDescription asChild>
              <div className='space-y-2'>
                <p>This starts a new run for only the tables that failed in this run:</p>
                <ul className='max-h-32 overflow-y-auto rounded-md border border-border bg-muted/40 px-2 py-1 font-mono text-xs'>
                  {failedTables.map((t) => (
                    <li key={t} className='truncate'>
                      {t}
                    </li>
                  ))}
                </ul>
                <p className='text-xs'>
                  Safe for <span className='font-medium'>replace</span> and{" "}
                  <span className='font-medium'>merge</span> tables. For{" "}
                  <span className='font-medium'>append</span> tables, rows committed before the
                  failure may be loaded again (at-least-once) — prefer merge/replace if that
                  matters.
                </p>
              </div>
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction
              onClick={async () => {
                setOpen(false);
                await onConfirm(failedTables);
              }}
            >
              Start run
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </>
  );
};

export default RetryFailedTablesButton;
