import PageHeader from "@/components/PageHeader";
import { Separator } from "@/components/ui/shadcn/separator";
import OutputLogs from "@/components/workflow/output/Logs";
import queryKeys from "@/hooks/api/queryKey";
import { service } from "@/services/service";
import { LogItem } from "@/services/types";
import { ThreadItem } from "@/types/chat";
import { useQueryClient } from "@tanstack/react-query";
import throttle from "lodash/throttle";
import { Workflow } from "lucide-react";
import { useEffect } from "react";
import { useCallback } from "react";
import { useRef } from "react";
import { useState } from "react";

const WorkflowThread = ({ thread }: { thread: ThreadItem }) => {
  const queryClient = useQueryClient();

  const [logs, setLogs] = useState<LogItem[]>([]);
  const hasRun = useRef(false);
  const [isPending, setIsPending] = useState(false);

  const appendLogs = useCallback((newLogs: LogItem[]) => {
    setLogs((prev) => [...prev, ...newLogs]);
  }, []);

  const processLogs = useCallback(() => {
    let buffer: LogItem[] = [];
    const flushLogs = throttle(
      () => {
        const logsToAppend = [...buffer];
        appendLogs(logsToAppend);
        buffer = [];
      },
      500,
      { leading: true, trailing: true },
    );

    return (logItem: LogItem) => {
      buffer.push(logItem);
      flushLogs();
    };
  }, [appendLogs]);

  useEffect(() => {
    if (hasRun.current) {
      return;
    }

    hasRun.current = true;

    if (thread.output) {
      setLogs(JSON.parse(thread.output));
      return;
    }

    const onLogItem = processLogs();

    setIsPending(true);
    setLogs([]);

    service
      .runWorkflowThread(thread.id, onLogItem)
      .finally(() => {
        setIsPending(false);
        queryClient.invalidateQueries({
          queryKey: queryKeys.thread.list(),
          type: "all",
        });
      })
      .catch((error) => {
        console.error("Error running workflow thread:", error);
        setIsPending(false);
        queryClient.invalidateQueries({
          queryKey: queryKeys.thread.list(),
          type: "all",
        });
      });
  }, [queryClient, thread, processLogs]);

  return (
    <div className="flex flex-col h-full">
      <PageHeader className="border-b-1 border-border items-center">
        <div className="p-2 flex items-center justify-center flex-1 h-full">
          <div className="flex gap-1 items-center text-muted-foreground">
            <Workflow className="w-4 h-4 min-w-4 min-h-4" />
            <p className="text-sm break-all">{thread?.source}</p>
          </div>
          <div className="px-4 h-full flex items-stretch">
            <Separator orientation="vertical" />
          </div>

          <p className="text-sm text-base-foreground">{thread?.title}</p>
        </div>
      </PageHeader>

      <div className="flex-1 w-full">
        <OutputLogs
          isPending={isPending}
          logs={logs}
          contentClassName="max-w-page-content mx-auto"
        />
      </div>
    </div>
  );
};

export default WorkflowThread;
