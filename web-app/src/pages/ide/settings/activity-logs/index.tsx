import { FileText } from "lucide-react";
import React from "react";
import { Table, TableBody, TableHead, TableHeader, TableRow } from "@/components/ui/shadcn/table";
import { useLogs } from "@/hooks/api/activityLogs/useLogs";
import PageHeader from "@/pages/ide/components/PageHeader";
import TableContentWrapper from "../components/TableContentWrapper";
import TableWrapper from "../components/TableWrapper";
import LogRow from "./LogRow";

export default function ActivityLogsPage() {
  const { data: logsResponse, isLoading: loading, error, refetch } = useLogs();

  const logs = React.useMemo(() => {
    return logsResponse?.logs || [];
  }, [logsResponse?.logs]);

  const sortedLogs = React.useMemo(() => {
    return logs.sort((a, b) => new Date(b.created_at).getTime() - new Date(a.created_at).getTime());
  }, [logs]);

  return (
    <div className='flex h-full flex-col'>
      <PageHeader
        icon={FileText}
        title='Activity Logs'
        description='View system audit logs and query history'
      />

      <div className='customScrollbar scrollbar-gutter-auto min-h-0 flex-1 overflow-auto p-4'>
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
                noFoundTitle='No logs found'
                noFoundDescription='There are currently no activity logs available.'
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
