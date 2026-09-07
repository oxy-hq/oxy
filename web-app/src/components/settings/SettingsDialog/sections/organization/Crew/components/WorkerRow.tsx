import { Badge } from "@/components/ui/shadcn/badge";
import { TableCell, TableRow } from "@/components/ui/shadcn/table";
import { assignmentLabel } from "@/libs/operatingGraph";
import type { AppAccessSummary } from "@/types/appAccess";
import type { FrontlineWorker } from "@/types/frontline";
import { workerStanding } from "../utils";
import { WorkerRowActions } from "./WorkerRowActions";
import { WorkerStatusBadge } from "./WorkerStatusBadge";

const CELL = "px-4 py-3 max-md:px-0 max-md:py-0";

export function WorkerRow({
  worker,
  orgId,
  apps,
  appsById
}: {
  worker: FrontlineWorker;
  orgId: string;
  apps: AppAccessSummary[];
  appsById: Map<string, AppAccessSummary>;
}) {
  const appNames = worker.apps.flatMap((id) => {
    const app = appsById.get(id);
    return app ? [app.name] : [];
  });

  return (
    <TableRow data-testid={`settings-crew-worker-${worker.identifier}`}>
      <TableCell data-label='Name' className={CELL}>
        <div className='flex items-center gap-3'>
          <div className='flex h-8 w-8 shrink-0 items-center justify-center rounded-full bg-muted font-medium text-sm'>
            {worker.name[0]?.toUpperCase() ?? "?"}
          </div>
          <span className='font-medium text-sm leading-none'>{worker.name}</span>
        </div>
      </TableCell>
      <TableCell
        data-label='Identifier'
        className={`${CELL} font-mono text-muted-foreground text-xs`}
      >
        {worker.identifier}
      </TableCell>
      <TableCell data-label='Status' className={CELL}>
        <WorkerStatusBadge standing={workerStanding(worker)} />
      </TableCell>
      <TableCell data-label='Apps' className={CELL}>
        {appNames.length > 0 ? (
          <div className='flex flex-wrap gap-1'>
            {appNames.map((name) => (
              <Badge key={name} variant='outline' className='font-normal text-muted-foreground'>
                {name}
              </Badge>
            ))}
          </div>
        ) : worker.apps.length > 0 ? (
          // Ids we can't name yet — the apps list is still loading, or the
          // grant points at an app that has since gone.
          <span className='text-muted-foreground text-xs'>
            {worker.apps.length} {worker.apps.length === 1 ? "app" : "apps"}
          </span>
        ) : (
          <span className='text-muted-foreground text-xs'>None</span>
        )}
      </TableCell>
      <TableCell data-label='Works at' className={CELL}>
        {worker.assignments.length > 0 ? (
          <div className='flex flex-wrap gap-1'>
            {worker.assignments.map((a) => (
              <Badge key={a.id} variant='outline' className='font-normal text-muted-foreground'>
                {assignmentLabel(a)}
              </Badge>
            ))}
          </div>
        ) : (
          <span className='text-muted-foreground text-xs'>None</span>
        )}
      </TableCell>
      <TableCell data-label='Enrolled' className={`${CELL} text-muted-foreground text-xs`}>
        {new Date(worker.created_at).toLocaleDateString()}
      </TableCell>
      <TableCell className='w-12 px-2 py-3 text-right max-md:w-auto max-md:px-0 max-md:py-0'>
        <WorkerRowActions worker={worker} orgId={orgId} apps={apps} />
      </TableCell>
    </TableRow>
  );
}
