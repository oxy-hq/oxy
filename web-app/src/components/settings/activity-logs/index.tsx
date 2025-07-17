import React from "react";
import { useLogs } from "@/hooks/api/activityLogs/useLogs";
import {
  Table,
  TableBody,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/shadcn/table";
import PageWrapper from "../components/PageWrapper";
import TableWrapper from "../components/TableWrapper";
import TableContentWrapper from "../components/TableContentWrapper";
import LogRow from "./LogRow";

const LogsManagement: React.FC = () => {
  const { data: logsResponse, isLoading: loading, error, refetch } = useLogs();

  const logs = React.useMemo(() => {
    return logsResponse?.logs || [];
  }, [logsResponse?.logs]);

  const sortedLogs = React.useMemo(() => {
    return logs.sort(
      (a, b) =>
        new Date(b.created_at).getTime() - new Date(a.created_at).getTime(),
    );
  }, [logs]);

  return (
    <PageWrapper title="Logs">
      <TableWrapper>
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>Thread</TableHead>
              <TableHead>Prompt</TableHead>
              <TableHead>Queries</TableHead>
              <TableHead>Created</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            <TableContentWrapper
              isEmpty={logs.length === 0}
              loading={loading}
              colSpan={4}
              error={error?.message}
              noFoundTitle="No logs found"
              noFoundDescription="There are currently no activity logs available."
              onRetry={refetch}
            >
              {sortedLogs.map((log) => (
                <LogRow key={log.id} log={log} />
              ))}
            </TableContentWrapper>
          </TableBody>
        </Table>
      </TableWrapper>
    </PageWrapper>
  );
};

export default LogsManagement;
