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

/**
 * "Retry failed tables" — re-runs only the resources that failed in a
 * run, by starting a new pipeline run scoped to those table names.
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

  const count = failedTables.length;
  return (
    <>
      <Button
        size={size}
        variant={variant}
        onClick={() => setOpen(true)}
        disabled={pending}
        aria-label='Retry failed tables'
      >
        {pending ? <Loader2 className='h-4 w-4 animate-spin' /> : <RefreshCw className='h-4 w-4' />}
        Retry failed ({count})
      </Button>
      <AlertDialog open={open} onOpenChange={setOpen}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>
              Retry {count} failed table{count === 1 ? "" : "s"}?
            </AlertDialogTitle>
            <AlertDialogDescription asChild>
              <div className='space-y-2'>
                <p>This starts a new run scoped to only the failed tables:</p>
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
              Retry
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </>
  );
};

export default RetryFailedTablesButton;
