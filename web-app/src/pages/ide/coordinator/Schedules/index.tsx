import { Plus, RefreshCw } from "lucide-react";
import type React from "react";
import { useState } from "react";
import { CanWorkspaceAdmin } from "@/components/auth/Can";
import { Button } from "@/components/ui/shadcn/button";
import { useSchedules } from "@/hooks/api/schedules/useSchedules";
import ScheduleDialog from "./ScheduleDialog";
import ScheduleTable from "./ScheduleTable";

/**
 * Coordinator → Schedules tab. Was previously under Settings → Workspace,
 * moved here because schedules are operational state (recurring runtime
 * triggers that produce runs) — they belong with Active Runs / Run
 * History / Recovery / Queue Health, not with workspace configuration.
 *
 * The CRUD components themselves (`ScheduleTable`, `ScheduleDialog`,
 * `ScheduleTable/ScheduleRow`) were lifted from the settings tree
 * verbatim; only this page chrome (and the route mount) is new.
 */
const SchedulesPage: React.FC = () => {
  const [createOpen, setCreateOpen] = useState(false);
  const { refetch, data } = useSchedules();
  const count = data?.length ?? 0;

  return (
    <CanWorkspaceAdmin
      fallback={
        <div className='flex h-full items-center justify-center'>
          <p className='text-muted-foreground text-sm'>
            You need workspace admin access to manage schedules.
          </p>
        </div>
      }
    >
      <div className='flex h-full flex-col'>
        <div className='flex items-center justify-between border-border border-b px-4 py-3'>
          <div>
            <h2 className='font-semibold text-base'>Schedules</h2>
            <p className='text-muted-foreground text-xs'>
              {count === 0
                ? "No schedules yet"
                : `${count} ${count === 1 ? "schedule" : "schedules"}`}
            </p>
          </div>
          <div className='flex items-center gap-2'>
            <Button variant='ghost' size='icon' onClick={() => refetch()} className='h-8 w-8'>
              <RefreshCw className='h-4 w-4' />
            </Button>
            <Button size='sm' variant='outline' onClick={() => setCreateOpen(true)}>
              <Plus className='h-4 w-4' />
              Create
            </Button>
          </div>
        </div>

        <div className='flex-1 overflow-y-auto p-4'>
          <ScheduleTable />
        </div>

        <ScheduleDialog open={createOpen} onOpenChange={setCreateOpen} />
      </div>
    </CanWorkspaceAdmin>
  );
};

export default SchedulesPage;
