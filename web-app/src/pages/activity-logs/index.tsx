import React from "react";
import { Clock, FileText, MessagesSquare } from "lucide-react";
import { useLogs } from "@/hooks/api/activityLogs/useLogs";
import { Link } from "react-router-dom";
import { Badge } from "@/components/ui/shadcn/badge";
import QueryContent from "./QueryContent";
import { formatDate } from "@/libs/utils/date";

const LogsManagement: React.FC = () => {
  const { data: logsResponse, isLoading: loading } = useLogs();

  const logs = React.useMemo(() => {
    return logsResponse?.logs || [];
  }, [logsResponse?.logs]);

  const groupedLogs = React.useMemo(() => {
    const groups: Record<string, typeof logs> = {};
    logs.forEach((log) => {
      if (!groups[log.thread_id]) {
        groups[log.thread_id] = [];
      }
      groups[log.thread_id].push(log);
    });

    Object.keys(groups).forEach((threadId) => {
      groups[threadId].sort(
        (a, b) =>
          new Date(b.created_at).getTime() - new Date(a.created_at).getTime(),
      );
    });

    return groups;
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
      <div className="h-full flex flex-col">
        <div className="max-w-6xl w-full mx-auto flex items-center justify-between p-6">
          <div className="flex items-center space-x-3">
            <FileText className="h-6 w-6" />
            <div>
              <h1 className="text-xl font-semibold">System Logs</h1>
              <p className="text-sm text-muted-foreground">
                Monitor system activities and user interactions
              </p>
            </div>
          </div>
        </div>

        <div className="overflow-auto flex-1 min-h-0 customScrollbar">
          <div className="max-w-6xl w-full mx-auto space-y-6 p-6">
            {Object.keys(groupedLogs).length === 0 ? (
              <div className="border rounded-lg p-8 text-center">
                <div className="text-muted-foreground">
                  <p>No logs found</p>
                  <p className="text-sm mt-1">
                    Logs will appear here when activities are recorded
                  </p>
                </div>
              </div>
            ) : (
              Object.entries(groupedLogs).map(([threadId, threadLogs]) => (
                <div key={threadId} className="border rounded-lg">
                  <div className="bg-secondary px-4 py-3 border-b flex items-center justify-between">
                    <h3 className="flex items-center gap-2">
                      <div className="p-2 bg-white dark:bg-slate-800 rounded-lg shadow-sm">
                        <MessagesSquare className="h-5 w-5 text-blue-600 dark:text-blue-400" />
                      </div>
                      <Link
                        to={`/threads/${threadId}`}
                        className="text-lg font-semibold text-slate-900 dark:text-white hover:text-blue-600 dark:hover:text-blue-400 transition-colors duration-200"
                      >
                        {threadLogs[0].thread?.title || "Untitled Thread"}
                      </Link>
                    </h3>
                    <Badge variant="outline">
                      {threadLogs.length} log
                      {threadLogs.length !== 1 ? "s" : ""}
                    </Badge>
                  </div>
                  {threadLogs.map((log) => (
                    <div
                      className="p-4 border-b last:border-0 space-y-2"
                      key={log.id}
                    >
                      <div className="flex items-start justify-between">
                        <p className="leading-relaxed whitespace-pre-wrap">
                          {log.prompts || "No prompt provided"}
                        </p>
                        <div className="flex items-center gap-2 text-xs text-muted-foreground">
                          <Clock className="h-3 w-3" />
                          {formatDate(log.created_at)}
                        </div>
                      </div>

                      <QueryContent log={log} />
                    </div>
                  ))}
                </div>
              ))
            )}
          </div>
        </div>
      </div>
    </div>
  );
};

export default LogsManagement;
