/**
 * Per-resource grid for an airway run — the centerpiece, matching the
 * Fivetran/Airbyte/dlt shape. One row per resource; relational
 * child tables nest directly under their parent.
 *
 * Presentation only — driven by `AirwayRunView.resources`.
 */

import type React from "react";

import { Badge } from "@/components/ui/shadcn/badge";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow
} from "@/components/ui/shadcn/table";
import { cn } from "@/libs/shadcn/utils";
import type { ResourceRow, ResourceStatus } from "@/utils/airwayReducer";

const STATUS_LABEL: Record<ResourceStatus, string> = {
  pending: "pending",
  extracting: "extracting",
  normalizing: "normalizing",
  loading: "loading",
  done: "done",
  error: "error"
};

const STATUS_VARIANT: Record<ResourceStatus, "default" | "secondary" | "destructive" | "outline"> =
  {
    pending: "outline",
    extracting: "secondary",
    normalizing: "secondary",
    loading: "secondary",
    done: "default",
    error: "destructive"
  };

const num = (n?: number) => (n == null ? "—" : n.toLocaleString());

const StatusBadge: React.FC<{ status: ResourceStatus }> = ({ status }) => (
  <Badge variant={STATUS_VARIANT[status]}>{STATUS_LABEL[status]}</Badge>
);

type Props = {
  resources: ResourceRow[];
};

export const ResourceGrid: React.FC<Props> = ({ resources }) => {
  if (resources.length === 0) {
    return (
      <div className='px-4 py-10 text-center text-muted-foreground text-sm'>
        No resources yet — waiting for the first extract.
      </div>
    );
  }

  return (
    <Table>
      <TableHeader>
        <TableRow>
          <TableHead>Resource</TableHead>
          <TableHead className='text-right'>Extracted</TableHead>
          <TableHead className='text-right'>Normalized</TableHead>
          <TableHead className='text-right'>Loaded</TableHead>
          <TableHead>Status</TableHead>
        </TableRow>
      </TableHeader>
      <TableBody>
        {resources.map((r) => {
          const isChild = !!r.parent;
          return (
            <TableRow key={`${r.parent ?? ""}:${r.table}`}>
              <TableCell className={cn("font-medium", isChild && "pl-8 text-muted-foreground")}>
                {isChild ? `└ ${r.table}` : r.table}
              </TableCell>
              <TableCell className='text-right tabular-nums'>{num(r.rowsExtracted)}</TableCell>
              <TableCell className='text-right tabular-nums'>{num(r.rowsNormalized)}</TableCell>
              <TableCell className='text-right tabular-nums'>{num(r.rowsLoaded)}</TableCell>
              <TableCell>
                <StatusBadge status={r.status} />
              </TableCell>
            </TableRow>
          );
        })}
      </TableBody>
    </Table>
  );
};

export default ResourceGrid;
