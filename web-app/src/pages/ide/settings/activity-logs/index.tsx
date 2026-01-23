import React from "react";
import { useLogs } from "@/hooks/api/activityLogs/useLogs";
import {
  Table,
  TableBody,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/shadcn/table";
import { FileText } from "lucide-react";
import LogRow from "./LogRow";
import PageHeader from "@/pages/ide/components/PageHeader";
import TableWrapper from "../components/TableWrapper";
import TableContentWrapper from "../components/TableContentWrapper";

export default function ActivityLogsPage() {
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
    <div className="flex flex-col h-full">
      <PageHeader
        icon={FileText}
        title="Activity Logs"
        description="View system audit logs and query history"
      />

      <div className="p-4 flex-1 overflow-auto min-h-0 customScrollbar scrollbar-gutter-auto">
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
                noFoundTitle="No logs found"
                noFoundDescription="There are currently no activity logs available."
                error={error?.message}
                onRetry={() => refetch()}
              >
                {sortedLogs.map((log) => (
                  <LogRow key={log.id} log={log} />
                ))}
              </TableContentWrapper>
            </TableBody>
          </Table>
        </TableWrapper>
      </div>
    </div>
  );
}
