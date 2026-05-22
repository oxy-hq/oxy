import type React from "react";
import TableContentWrapper from "@/components/settings/components/TableContentWrapper";
import TableWrapper from "@/components/settings/components/TableWrapper";
import { Table, TableBody, TableHead, TableHeader, TableRow } from "@/components/ui/shadcn/table";
import { useSchedules } from "@/hooks/api/schedules/useSchedules";
import ScheduleRow from "./ScheduleRow";

const ScheduleTable: React.FC = () => {
  const { data, isLoading, error, refetch } = useSchedules();
  const schedules = data ?? [];
  return (
    <TableWrapper>
      <Table>
        <TableHeader>
          <TableRow>
            <TableHead>Name</TableHead>
            <TableHead>Target</TableHead>
            <TableHead>Schedule</TableHead>
            <TableHead>Next run</TableHead>
            <TableHead>Last run</TableHead>
            <TableHead>Enabled</TableHead>
            <TableHead>Actions</TableHead>
          </TableRow>
        </TableHeader>
        <TableBody>
          <TableContentWrapper
            isEmpty={schedules.length === 0}
            loading={isLoading}
            colSpan={7}
            error={error?.message}
            noFoundTitle='No schedules'
            noFoundDescription='Create a schedule to run a workflow or airway pipeline on a recurring cron.'
            onRetry={refetch}
          >
            {schedules.map((s) => (
              <ScheduleRow key={s.id} schedule={s} />
            ))}
          </TableContentWrapper>
        </TableBody>
      </Table>
    </TableWrapper>
  );
};

export default ScheduleTable;
