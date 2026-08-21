import { TableCell, TableRow } from "@/components/ui/shadcn/table";
import { AdminStatusPill } from "@/pages/admin/components/AdminStatusPill";
import { ADMIN_ROW_CLASS } from "@/pages/admin/components/AdminTable";
import { CopyableId } from "@/pages/admin/components/CopyableId";
import { relativeTime } from "@/pages/admin/utils";
import type { AirhouseFleetRow as Row } from "@/services/api/airhouseAdmin";
import { severityOf } from "../severity";

/**
 * One provisioned warehouse, as a row in the shared operator table.
 *
 * Uses `AdminTable`'s treatment rather than hand-rolled divs. That component
 * exists because every admin list used to style its own rows and drift, and this
 * page was the newest copy of the drift. It also buys the column headers the div
 * version never had — a 56-character monospace column with no label is a
 * guessing game on a page read under time pressure.
 *
 * Row grammar unchanged: identifiers left, anything that can need attention
 * right, so an operator who knows one admin page can read this one.
 *
 * `tenant_id` is a `CopyableId` because the next thing an operator does with it
 * is paste it into a terminal.
 */
export const AirhouseFleetRow = ({ row }: { row: Row }) => (
  <TableRow
    className={ADMIN_ROW_CLASS}
    data-testid={`admin-airhouse-row-${row.workspace_id}`}
    data-severity={severityOf(row)}
  >
    <TableCell className='py-1 font-medium text-xs'>
      <span className='block max-w-48 truncate' title={row.workspace_name}>
        {row.workspace_name}
      </span>
    </TableCell>
    <TableCell className='py-1 text-muted-foreground text-xs'>
      <span className='block max-w-36 truncate' title={row.org_name}>
        {row.org_name}
      </span>
    </TableCell>
    <TableCell className='py-1'>
      <CopyableId value={row.tenant_id} head={12} />
    </TableCell>
    <TableCell className='py-1 text-muted-foreground text-xs'>
      <span
        className='block max-w-64 truncate font-mono'
        title={`${row.bucket}${row.prefix ? `/${row.prefix}` : ""}`}
      >
        {row.bucket}
        {row.prefix ? `/${row.prefix}` : ""}
      </span>
    </TableCell>
    {/* Fetched by the API since day one and never rendered. Rotation age is the
        first thing asked about a credential that stopped working, and "never"
        is itself a finding. */}
    <TableCell className='py-1 text-right text-muted-foreground text-xs tabular-nums'>
      {relativeTime(row.sa_rotated_at)}
    </TableCell>
    <TableCell className='py-1 text-right'>
      <span className='flex items-center justify-end gap-1.5'>
        {/* A tenant without a usable service account is provisioned in name
            only — it cannot mint the ephemeral credential every query needs, and
            nothing says so until someone runs one. */}
        {!row.service_account_ready && <AdminStatusPill tone='danger' label='no service account' />}
        <AdminStatusPill tone={row.status === "active" ? "ok" : "warn"} label={row.status} />
      </span>
    </TableCell>
  </TableRow>
);
