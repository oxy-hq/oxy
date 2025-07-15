import React from "react";
import { useLogs } from "@/hooks/api/activityLogs/useLogs";
import { Link } from "react-router-dom";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/shadcn/table";
import QueryContent from "./QueryContent";
import LogMetadata from "./QueryContent/LogMetadata";
import { formatDate } from "@/libs/utils/date";

const LogsManagement: React.FC = () => {
  const { data: logsResponse, isLoading: loading } = useLogs();

  const logs = React.useMemo(() => {
    return logsResponse?.logs || [];
  }, [logsResponse?.logs]);

  const sortedLogs = React.useMemo(() => {
    return logs.sort(
      (a, b) =>
        new Date(b.created_at).getTime() - new Date(a.created_at).getTime(),
    );
  }, [logs]);

  if (loading) {
    return (
      <div className="flex flex-col h-full">
        <div className="flex-1 p-6">
          <div className="max-w-6xl mx-auto">
            <div className="flex items-center justify-center h-64">
              <div className="text-lg">Loading logs...</div>
            </div>
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className="flex flex-col h-full">
      <div className="flex-1 p-6">
        <div className="max-w-6xl mx-auto">
          <div className="flex items-center justify-between mb-10 border-b pb-4">
            <h1 className="text-xl font-semibold">System Logs</h1>
          </div>

          <div className="border rounded-lg">
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Thread</TableHead>
                  <TableHead>Prompt</TableHead>
                  <TableHead>Query Details</TableHead>
                  <TableHead>Metadata</TableHead>
                  <TableHead>Created</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {(() => {
                  if (loading) {
                    return (
                      <TableRow>
                        <TableCell colSpan={5} className="text-center py-8">
                          Loading logs...
                        </TableCell>
                      </TableRow>
                    );
                  }

                  if (sortedLogs.length === 0) {
                    return (
                      <TableRow>
                        <TableCell colSpan={5} className="text-center py-8">
                          <div className="text-muted-foreground">
                            <p>No logs found</p>
                            <p className="text-sm mt-1">
                              Logs will appear here when activities are recorded
                            </p>
                          </div>
                        </TableCell>
                      </TableRow>
                    );
                  }

                  return sortedLogs.map((log) => (
                    <TableRow key={log.id}>
                      <TableCell className="max-w-md whitespace-pre-wrap break-words">
                        <Link
                          to={`/threads/${log.thread_id}`}
                          className="text-blue-600 dark:text-blue-400 hover:underline"
                        >
                          {log.thread?.title || "Untitled Thread"}
                        </Link>
                      </TableCell>
                      <TableCell>
                        <div className="whitespace-pre-wrap break-words">
                          {log.prompts || "No prompt provided"}
                        </div>
                      </TableCell>
                      <TableCell>
                        <QueryContent log={log} />
                      </TableCell>
                      <TableCell>
                        <LogMetadata log={log} />
                      </TableCell>
                      <TableCell className="whitespace-nowrap">
                        {formatDate(log.created_at)}
                      </TableCell>
                    </TableRow>
                  ));
                })()}
              </TableBody>
            </Table>
          </div>
        </div>
      </div>
    </div>
  );
};

export default LogsManagement;
