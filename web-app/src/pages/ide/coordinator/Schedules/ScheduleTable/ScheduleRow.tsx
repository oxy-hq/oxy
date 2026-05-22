import { AlertTriangle, Pencil, Play, Trash2 } from "lucide-react";
import type React from "react";
import { useState } from "react";
import { Badge } from "@/components/ui/shadcn/badge";
import { Button } from "@/components/ui/shadcn/button";
import { Switch } from "@/components/ui/shadcn/switch";
import { TableCell, TableRow } from "@/components/ui/shadcn/table";
import {
  useDeleteSchedule,
  useRunScheduleNow,
  useUpdateSchedule
} from "@/hooks/api/schedules/useSchedules";
import type { Schedule, ScheduleInput } from "@/types/schedule";
import ScheduleDialog from "../ScheduleDialog";
import DeleteScheduleDialog from "./DeleteScheduleDialog";

const fmt = (iso: string | null) => (iso ? new Date(iso).toLocaleString() : "—");

const toInput = (s: Schedule): ScheduleInput => ({
  name: s.name,
  target_kind: s.target_kind,
  target_ref: s.target_ref,
  variables: s.variables,
  cron_expr: s.cron_expr,
  timezone: s.timezone,
  enabled: s.enabled
});

const ScheduleRow: React.FC<{ schedule: Schedule }> = ({ schedule }) => {
  const [deleteOpen, setDeleteOpen] = useState(false);
  const [editOpen, setEditOpen] = useState(false);
  const del = useDeleteSchedule();
  const update = useUpdateSchedule();
  const runNow = useRunScheduleNow();

  const toggleEnabled = (enabled: boolean) =>
    update.mutate({ id: schedule.id, input: { ...toInput(schedule), enabled } });

  return (
    <>
      <TableRow>
        <TableCell data-label='Name'>
          <div className='flex items-center gap-2'>
            <span className='font-medium'>{schedule.name}</span>
            {schedule.last_error && (
              <span
                role='img'
                className='inline-flex'
                aria-label='Last run failed'
                title={`Last error: ${schedule.last_error}`}
              >
                <AlertTriangle className='h-4 w-4 shrink-0 text-destructive' />
              </span>
            )}
            {schedule.missed_runs > 0 && (
              <Badge
                variant='outline'
                title={
                  schedule.last_missed_at
                    ? `${schedule.missed_runs} occurrence${
                        schedule.missed_runs === 1 ? "" : "s"
                      } skipped (last detected ${new Date(
                        schedule.last_missed_at
                      ).toLocaleString()}). Policy: run-once-then-resume — one catch-up fired, the rest were not.`
                    : `${schedule.missed_runs} occurrence${
                        schedule.missed_runs === 1 ? "" : "s"
                      } skipped.`
                }
              >
                {schedule.missed_runs} missed
              </Badge>
            )}
          </div>
        </TableCell>
        <TableCell data-label='Target'>
          <div className='flex items-center gap-2'>
            <Badge variant='secondary'>{schedule.target_kind}</Badge>
            <span className='font-mono text-muted-foreground text-sm'>{schedule.target_ref}</span>
          </div>
        </TableCell>
        <TableCell data-label='Schedule'>
          <code className='font-mono text-sm'>{schedule.cron_expr}</code>
          <span className='text-muted-foreground text-xs'> {schedule.timezone}</span>
        </TableCell>
        <TableCell data-label='Next run'>{fmt(schedule.next_run_at)}</TableCell>
        <TableCell data-label='Last run'>{fmt(schedule.last_fired_at)}</TableCell>
        <TableCell data-label='Enabled'>
          <Switch
            checked={schedule.enabled}
            onCheckedChange={toggleEnabled}
            disabled={update.isPending}
          />
        </TableCell>
        <TableCell>
          <div className='flex items-center gap-1'>
            <Button
              variant='ghost'
              size='sm'
              title='Run now'
              disabled={runNow.isPending}
              onClick={() => runNow.mutate(schedule.id)}
            >
              <Play />
            </Button>
            <Button variant='ghost' size='sm' title='Edit' onClick={() => setEditOpen(true)}>
              <Pencil />
            </Button>
            <Button variant='ghost' size='sm' title='Delete' onClick={() => setDeleteOpen(true)}>
              <Trash2 className='!text-destructive' />
            </Button>
          </div>
        </TableCell>
      </TableRow>

      <ScheduleDialog open={editOpen} onOpenChange={setEditOpen} schedule={schedule} />
      <DeleteScheduleDialog
        open={deleteOpen}
        onOpenChange={setDeleteOpen}
        schedule={schedule}
        onConfirm={async () => {
          await del.mutateAsync(schedule.id);
          setDeleteOpen(false);
        }}
      />
    </>
  );
};

export default ScheduleRow;
