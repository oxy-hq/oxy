import { ChevronRight } from "lucide-react";
import type { KeyboardEvent } from "react";
import { TableCell, TableRow } from "@/components/ui/shadcn/table";
import { cn } from "@/libs/shadcn/utils";
import { AdminStatusPill } from "@/pages/admin/components/AdminStatusPill";
import { ADMIN_ROW_CLASS } from "@/pages/admin/components/AdminTable";
import { CopyableId } from "@/pages/admin/components/CopyableId";
import type { AirhouseFleetRow as Row } from "@/services/api/airhouseAdmin";
import { credentialAge } from "../credentialAge";
import { type Severity, severityOf } from "../severity";
import { AirhouseFleetRowDetail } from "./AirhouseFleetRowDetail";

/**
 * The leading edge of every row carries its severity as a rule, so the fleet's
 * health reads as one vertical line rather than a pill at the far right of six
 * columns. On a page opened during an incident the eye should not have to cross
 * the table to learn which rows matter.
 */
const RAIL: Record<Severity, string> = {
  broken: "bg-destructive",
  degraded: "bg-warning",
  healthy: "bg-border"
};

/**
 * One provisioned warehouse: a dense row that opens in place.
 *
 * **Expand-in-place, not a side panel.** A panel costs horizontal room for as
 * long as it is open, and this page's job is to show as much of the fleet at
 * once as it can. Opening downward spends space only on the row being
 * investigated, and keeps its neighbours on screen for comparison — which is
 * most of what diagnosing one tenant against the fleet consists of.
 *
 * **The whole row is the control**, and it is reachable from the keyboard: an
 * operator could tab to every `CopyableId` inside the row but not to the thing
 * that opens the strip those ids sit above. `CopyableId` stops its own click
 * propagating, so copying an id does not also toggle the row.
 *
 * Row grammar unchanged: identifiers left, anything that can need attention
 * right, so an operator who knows one admin page can read this one.
 */
export const AirhouseFleetRow = ({
  row,
  expanded,
  onToggle
}: {
  row: Row;
  expanded: boolean;
  onToggle: () => void;
}) => {
  const severity = severityOf(row);
  const age = credentialAge(row);
  const storage = `${row.bucket}${row.prefix ? `/${row.prefix}` : ""}`;

  const onKeyDown = (e: KeyboardEvent<HTMLTableRowElement>) => {
    // The row itself, not something focused inside it. Enter or Space on a
    // nested `CopyableId` is that button's own activation, and a `preventDefault`
    // here cancels the click a `<button>` turns the key into — so the id was
    // never copied, silently, and the row toggled instead. `CopyableId` stops
    // its own *click* from reaching the row, which makes the mouse safe and
    // says nothing about the keyboard.
    if (e.target !== e.currentTarget) return;
    if (e.key !== "Enter" && e.key !== " ") return;
    // Space scrolls the page otherwise, which moves the row out from under the
    // operator at the moment they open it.
    e.preventDefault();
    onToggle();
  };

  return (
    <>
      <TableRow
        className={cn(
          ADMIN_ROW_CLASS,
          "outline-none focus-visible:ring-2 focus-visible:ring-ring/50",
          expanded && "bg-muted/40"
        )}
        data-testid={`admin-airhouse-row-${row.workspace_id}`}
        data-severity={severity}
        data-expanded={expanded}
        tabIndex={0}
        aria-expanded={expanded}
        onClick={onToggle}
        onKeyDown={onKeyDown}
      >
        <TableCell className='relative py-1 pl-3 font-medium text-xs'>
          <span className={cn("absolute inset-y-0 left-0 w-0.5", RAIL[severity])} />
          <span className='flex items-center gap-1'>
            <ChevronRight
              className={cn(
                "size-3 shrink-0 text-muted-foreground transition-transform",
                expanded && "rotate-90"
              )}
            />
            <span className='block max-w-44 truncate' title={row.workspace_name}>
              {row.workspace_name}
            </span>
          </span>
        </TableCell>
        <TableCell className='py-1 text-muted-foreground text-xs'>
          <span className='block max-w-32 truncate' title={row.org_name}>
            {row.org_name}
          </span>
        </TableCell>
        <TableCell className='py-1'>
          <CopyableId value={row.tenant_id} head={12} />
        </TableCell>
        <TableCell className='py-1 text-muted-foreground text-xs'>
          <span className='block max-w-56 truncate font-mono' title={storage}>
            {storage}
          </span>
        </TableCell>
        {/* Rotation age, with the account's own age folded in — see
            `credentialAge`. "never" alone reads the same for a tenant made this
            morning and one made two years ago. */}
        <TableCell
          className={cn(
            "py-1 text-right text-xs tabular-nums",
            age.overdue ? "text-warning" : "text-muted-foreground"
          )}
        >
          {age.label}
        </TableCell>
        <TableCell className='py-1 text-right'>
          <span className='flex items-center justify-end gap-1.5'>
            {/* A tenant without a usable service account is provisioned in name
                only — it cannot mint the ephemeral credential every query
                needs, and nothing says so until someone runs one. */}
            {!row.service_account_ready && (
              <AdminStatusPill tone='danger' label='no service account' />
            )}
            <AdminStatusPill tone={row.status === "active" ? "ok" : "warn"} label={row.status} />
          </span>
        </TableCell>
      </TableRow>

      {expanded && <AirhouseFleetRowDetail row={row} rail={RAIL[severity]} />}
    </>
  );
};
