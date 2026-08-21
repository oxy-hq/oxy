import { Search, Warehouse } from "lucide-react";
import { useMemo, useState } from "react";
import { Input } from "@/components/ui/shadcn/input";
import { Skeleton } from "@/components/ui/shadcn/skeleton";
import { Table, TableBody, TableHeader, TableRow } from "@/components/ui/shadcn/table";
import { useAirhouseFleet } from "@/hooks/api/airhouse/useAdminAirhouse";
import { AdminEmptyState } from "../components/AdminEmptyState";
import { AdminSectionLabel } from "../components/AdminSectionLabel";
import { ADMIN_HEADER_ROW_CLASS, AdminTh } from "../components/AdminTable";
import { AirhouseFleetRow } from "./components/AirhouseFleetRow";
import { AirhouseUnprovisionedRow } from "./components/AirhouseUnprovisionedRow";
import { FleetFilterChips } from "./components/FleetFilterChips";
import { bySeverityThenName, countBySeverity, type Severity, severityOf } from "./severity";

/**
 * The Airhouse fleet: which workspaces have a warehouse, and is anything wrong.
 *
 * **A triage queue, not a list.** The page is read when something is broken, so
 * it is ordered by severity rather than by name: the tenants that cannot serve a
 * query are on screen before the operator does anything. Filtering is the same
 * decision — the summary counts *are* the filter, so "2 without a service
 * account" is one click from those two rows instead of a number to go hunting
 * with.
 *
 * **Workspace-keyed.** An Airhouse tenant is one per workspace rather than one
 * per org, so the org is a column here rather than the key.
 *
 * There is no detail panel yet: everything an operator can currently *do* is
 * provision, and everything they can read fits in the row. Catalog indexes and
 * service-account rotation are the next things worth opening a panel for.
 */
const AdminAirhouse = () => {
  const { data, isPending, isError, error } = useAirhouseFleet();
  const [query, setQuery] = useState("");
  const [severity, setSeverity] = useState<Severity | null>(null);

  const { live, empty, counts, provisionedTotal, total } = useMemo(() => {
    const all = data?.rows ?? [];
    const q = query.trim().toLowerCase();
    const match = (r: (typeof all)[number]) =>
      !q ||
      r.workspace_name.toLowerCase().includes(q) ||
      r.org_name.toLowerCase().includes(q) ||
      r.tenant_id.toLowerCase().includes(q) ||
      r.bucket.toLowerCase().includes(q);

    const provisioned = all.filter((r) => r.status !== "none");
    return {
      // Counts come from the whole provisioned fleet, not the filtered view —
      // a chip that renumbered itself as you filtered would make the fleet look
      // like it changed when only the question did.
      counts: countBySeverity(provisioned),
      provisionedTotal: provisioned.length,
      total: all.length,
      live: provisioned
        .filter(match)
        .filter((r) => severity === null || severityOf(r) === severity)
        .sort(bySeverityThenName),
      empty: all.filter((r) => r.status === "none" && match(r))
    };
  }, [data, query, severity]);

  // `FleetTruncation.any()` is a Rust method and does not cross the wire.
  const anyTruncated = Boolean(data?.truncated?.unprovisioned || data?.truncated?.provisioned);

  if (isPending) return <Skeleton className='m-4 h-64' />;
  if (isError) {
    return (
      <AdminEmptyState
        icon={Warehouse}
        title='Could not list Airhouse tenants'
        description={error instanceof Error ? error.message : undefined}
      />
    );
  }

  return (
    <div className='flex flex-col gap-3 p-4' data-testid='admin-airhouse'>
      <div className='flex flex-wrap items-center justify-between gap-x-4 gap-y-2'>
        <div className='flex flex-wrap items-center gap-x-3 gap-y-1'>
          <h1 className='font-semibold text-xl tracking-tight'>Airhouse warehouses</h1>
          <span className='text-muted-foreground text-xs tabular-nums'>
            {provisionedTotal} of {anyTruncated ? `the first ${total}` : total} workspaces
            provisioned
          </span>
        </div>
        <div className='relative w-64'>
          <Search className='absolute top-1/2 left-2 size-3 -translate-y-1/2 text-muted-foreground' />
          <Input
            className='h-7 pl-7 text-xs'
            placeholder='Workspace, org, tenant or bucket…'
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            data-testid='admin-airhouse-filter'
          />
        </div>
      </div>

      {/* Report and act in one control — see FleetFilterChips. */}
      <FleetFilterChips
        counts={counts}
        total={provisionedTotal}
        active={severity}
        onChange={setSeverity}
      />

      {/* Which half was cut decides the words. The cap normally falls on
          workspaces without a warehouse, so every provisioned tenant is on the
          page — but when the provisioned half hits its own cap, a warehouse may
          be missing, and the first sentence would assert the opposite of what
          happened.

          The filter is client-side: it searches the rows already returned, so
          telling an operator to "narrow it" would point them at the one action
          that cannot reach a truncated row, and would look like it worked. */}
      {(data?.truncated?.provisioned || data?.truncated?.unprovisioned) && (
        <p className='text-warning text-xs' data-testid='admin-airhouse-truncated'>
          {data.truncated.provisioned
            ? "More rows exist than are shown, including workspaces that have a warehouse."
            : `Every provisioned workspace is shown; the ones without a warehouse are the first ${total - provisionedTotal} by name.`}
        </p>
      )}

      <div className='flex flex-col gap-2' data-testid='admin-airhouse-provisioned'>
        <AdminSectionLabel trailing={String(live.length)}>Provisioned</AdminSectionLabel>
        {live.length === 0 ? (
          <p
            className='px-1 py-2 text-muted-foreground text-xs'
            data-testid='admin-airhouse-provisioned-empty'
          >
            {severity || query
              ? "No warehouse matches that filter."
              : "No workspace has a warehouse yet."}
          </p>
        ) : (
          <Table>
            <TableHeader>
              <TableRow className={ADMIN_HEADER_ROW_CLASS}>
                <AdminTh>Workspace</AdminTh>
                <AdminTh>Org</AdminTh>
                <AdminTh>Tenant</AdminTh>
                <AdminTh>Storage</AdminTh>
                <AdminTh align='right'>SA rotated</AdminTh>
                <AdminTh align='right'>Status</AdminTh>
              </TableRow>
            </TableHeader>
            <TableBody>
              {live.map((r) => (
                <AirhouseFleetRow key={r.workspace_id} row={r} />
              ))}
            </TableBody>
          </Table>
        )}
      </div>

      {/* Hidden while a severity filter is on: severity is a property of a
          provisioned tenant, so a list of workspaces that have none is not an
          answer to "show me the broken ones" — it is the rest of the page
          refusing to narrow. */}
      {severity === null && empty.length > 0 && (
        <div className='flex flex-col gap-2' data-testid='admin-airhouse-unprovisioned'>
          <AdminSectionLabel trailing={String(empty.length)}>No warehouse</AdminSectionLabel>
          <div className='grid grid-cols-1 gap-x-6 md:grid-cols-2 xl:grid-cols-3'>
            {empty.map((r) => (
              <AirhouseUnprovisionedRow key={r.workspace_id} row={r} />
            ))}
          </div>
        </div>
      )}
    </div>
  );
};

export default AdminAirhouse;
