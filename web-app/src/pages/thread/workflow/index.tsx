import PageHeader from "@/components/PageHeader";
import { Separator } from "@/components/ui/shadcn/separator";
import queryKeys from "@/hooks/api/queryKey";
import { LogItem } from "@/hooks/api/runWorkflow";
import runWorkflowThread from "@/hooks/api/runWorkflowThread";
import OutputLogs from "@/pages/workflow/output/Logs";
import { ThreadItem } from "@/types/chat";
import { useQueryClient } from "@tanstack/react-query";
import { throttle } from "lodash";
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

  const processLogs = useCallback(
    async (data: AsyncGenerator<LogItem, void, unknown> | undefined) => {
      if (!data) return;
      let buffer: LogItem[] = [];
      const flushLogs = throttle(
        () => {
          const logsToAppend = [...buffer];
          setLogs((prev) => [...prev, ...logsToAppend]);
          buffer = [];
        },
        500,
        { leading: true, trailing: true },
      );

      for await (const logItem of data) {
        buffer.push(logItem);
        flushLogs();
      }
    },
    [],
  );

  useEffect(() => {
    if (hasRun.current) {
      return;
    }

    hasRun.current = true;

    if (thread.output) {
      setLogs(JSON.parse(thread.output));
      return;
    }
    // eslint-disable-next-line promise/catch-or-return
    runWorkflowThread({ threadId: thread.id })
      .then(async (data) => {
        setIsPending(true);
        setLogs([]);
        return processLogs(data);
      })
      .finally(() => {
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
          contentClassName="max-w-[742px] mx-auto"
        />
      </div>
    </div>
  );
};

export default WorkflowThread;
