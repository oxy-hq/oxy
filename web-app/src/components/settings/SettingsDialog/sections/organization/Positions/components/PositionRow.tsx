import { TableCell, TableRow } from "@/components/ui/shadcn/table";
import { SCOPE_LABELS } from "@/libs/operatingGraph";
import type { RoleRow } from "@/types/operatingGraph";
import { heldByLabel } from "../utils";
import { PositionRowActions } from "./PositionRowActions";

const CELL = "px-4 py-3 max-md:px-0 max-md:py-0";

export function PositionRow({
  orgId,
  role,
  holders
}: {
  orgId: string;
  role: RoleRow;
  /** Distinct people holding it, or undefined while assignments load. */
  holders: number | undefined;
}) {
  return (
    <TableRow data-testid={`settings-positions-row-${role.id}`}>
      <TableCell data-label='Name' className={CELL}>
        <span className='font-medium text-sm'>{role.name}</span>
      </TableCell>
      <TableCell data-label='Scope' className={`${CELL} text-sm`}>
        {SCOPE_LABELS[role.scope]}
      </TableCell>
      <TableCell data-label='Held by' className={`${CELL} text-muted-foreground text-sm`}>
        {holders === undefined ? "…" : heldByLabel(holders)}
      </TableCell>
      <TableCell className='w-12 px-2 py-3 text-right max-md:w-auto max-md:px-0 max-md:py-0'>
        <PositionRowActions orgId={orgId} role={role} holders={holders ?? 0} />
      </TableCell>
    </TableRow>
  );
}
