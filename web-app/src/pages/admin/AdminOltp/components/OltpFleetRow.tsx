import { cn } from "@/libs/shadcn/utils";
import { AdminStatusPill } from "@/pages/admin/components/AdminStatusPill";
import { OltpSchemaStrip } from "@/pages/admin/components/OltpSchemaStrip";
import type { OltpTenantRow } from "@/services/api/oltp";

/**
 * One provisioned tenant.
 *
 * A row, not a table cell grid: the six-column table this replaces spent most
 * of its width on gaps between short values, and three of those columns held
 * `—` for the majority of rows. The identifiers sit together on the left in
 * monospace (they are `psql` arguments, not prose) and everything that can need
 * attention sits together on the right, so a scan down the right edge answers
 * "is anything wrong" without reading a single database name.
 *
 * Selects rather than navigates. Following the link took an operator to the org
 * page's OLTP *section*, below Branding, Identity and Members — leaving the
 * surface they came to use. The panel opens beside the list instead.
 */
export const OltpFleetRow = ({
  row,
  selected,
  onSelect,
  compact
}: {
  row: OltpTenantRow;
  selected: boolean;
  onSelect: () => void;
  /** The list is narrow because a tenant is open; drop to name + state. */
  compact?: boolean;
}) => {
  const trouble = !row.analyst_ready || row.platform_drift;
  return (
    <button
      type='button'
      onClick={onSelect}
      className={cn(
        "flex w-full items-center gap-3 border-border/60 border-b px-2 py-1.5 text-left transition-colors last:border-b-0 hover:bg-muted/40",
        selected && "bg-muted"
      )}
      data-testid={`admin-oltp-row-${row.org_id}`}
    >
      <span className={cn("shrink-0 truncate font-medium text-xs", compact ? "flex-1" : "w-44")}>
        {row.org_name}
      </span>

      {!compact && (
        <>
          <span
            className='w-64 shrink-0 truncate font-mono text-muted-foreground text-xs'
            title={`${row.database} · ${row.host}`}
          >
            {row.database}
          </span>
          <span className='w-28 shrink-0 truncate text-muted-foreground text-xs'>
            {row.provider}
            {row.region ? ` · ${row.region}` : ""}
          </span>
          <OltpSchemaStrip schemas={row.schemas} className='min-w-0 flex-1' />
        </>
      )}

      <span className='flex shrink-0 items-center gap-1.5'>
        {!row.analyst_ready && <AdminStatusPill tone='danger' label='no analyst' />}
        {row.platform_drift && <AdminStatusPill tone='warn' label='drift' />}
        {!trouble && (
          <AdminStatusPill tone={row.status === "active" ? "ok" : "warn"} label={row.status} />
        )}
      </span>
    </button>
  );
};
