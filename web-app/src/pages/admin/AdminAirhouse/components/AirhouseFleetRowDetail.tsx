import type { ReactNode } from "react";
import { TableCell, TableRow } from "@/components/ui/shadcn/table";
import { cn } from "@/libs/shadcn/utils";
import { CopyableId } from "@/pages/admin/components/CopyableId";
import type { AirhouseFleetRow as Row } from "@/services/api/airhouseAdmin";
import { ttlLabel } from "../credentialAge";

/** An absent value reads as absent, not as an empty cell. */
const Missing = () => <span className='text-muted-foreground'>—</span>;

/**
 * A labelled fact.
 *
 * Keyed by a stable `id` rather than its display copy, so a test asserts the
 * field is present and not the wording it currently has.
 */
const Fact = ({ id, label, children }: { id: string; label: string; children: ReactNode }) => (
  <div className='flex min-w-0 flex-col gap-0.5' data-testid={`admin-airhouse-fact-${id}`}>
    <span className='font-medium text-[10px] text-muted-foreground uppercase tracking-wider'>
      {label}
    </span>
    <span className='truncate text-xs'>{children}</span>
  </div>
);

/**
 * A date an operator can correlate with a log line.
 *
 * The collapsed row gives a magnitude, because scanning wants one; this gives
 * the day, because acting wants something to search for.
 */
function absoluteDay(iso: string | null): ReactNode {
  if (!iso) return <Missing />;
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return <Missing />;
  return d.toISOString().slice(0, 10);
}

/**
 * The strip a row opens into: the psql session an operator would otherwise
 * open.
 *
 * Every identifier they will paste somewhere, plus the service account's two
 * ceilings.
 *
 * **`Max role` and `Max lifetime` are ceilings, not the credential a caller
 * gets.** Both are written once at provisioning from constants and never
 * varied, so on a healthy fleet every row reads the same pair — the effective
 * role and TTL are chosen per mint by the broker from the caller's org role,
 * which this page does not carry. They earn their place by the inverse
 * reading: a row that does NOT match the rest of the fleet is a tenant
 * provisioned under an older policy, and that is a finding a glance can catch.
 */
export const AirhouseFleetRowDetail = ({ row, rail }: { row: Row; rail: string }) => (
  <TableRow className='border-border/60' data-testid={`admin-airhouse-detail-${row.workspace_id}`}>
    <TableCell colSpan={6} className='relative bg-muted/20 py-2 pl-3'>
      <span className={cn("absolute inset-y-0 left-0 w-0.5", rail)} />
      <div className='grid grid-cols-2 gap-x-6 gap-y-2 md:grid-cols-3 xl:grid-cols-5'>
        <Fact id='workspace-id' label='Workspace id'>
          <CopyableId value={row.workspace_id} head={8} />
        </Fact>
        <Fact id='org-id' label='Org id'>
          {row.org_id ? <CopyableId value={row.org_id} head={8} /> : <Missing />}
        </Fact>
        <Fact id='service-account' label='Service account'>
          {row.service_account_id ? (
            <CopyableId value={row.service_account_id} head={14} />
          ) : (
            <span className='text-destructive'>not bound</span>
          )}
        </Fact>
        <Fact id='role' label='Max role'>
          {row.bearer_max_role ?? <Missing />}
        </Fact>
        <Fact id='ttl' label='Max lifetime'>
          <span className='tabular-nums'>{ttlLabel(row.bearer_max_ttl_secs)}</span>
        </Fact>
        <Fact id='bucket' label='Bucket'>
          <span className='font-mono'>{row.bucket || <Missing />}</span>
        </Fact>
        <Fact id='prefix' label='Prefix'>
          <span className='font-mono'>{row.prefix || <Missing />}</span>
        </Fact>
        <Fact id='created' label='Provisioned'>
          <span className='tabular-nums'>{absoluteDay(row.created_at)}</span>
        </Fact>
        <Fact id='sa-created' label='Account bound'>
          <span className='tabular-nums'>{absoluteDay(row.sa_created_at)}</span>
        </Fact>
        <Fact id='sa-rotated' label='Last rotated'>
          <span className='tabular-nums'>{absoluteDay(row.sa_rotated_at)}</span>
        </Fact>
      </div>
    </TableCell>
  </TableRow>
);
