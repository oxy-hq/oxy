import PageHeader from "@/components/PageHeader";
import { Separator } from "@/components/ui/shadcn/separator";
import OutputLogs from "@/components/workflow/output/Logs";
import useWorkflowThreadStore from "@/stores/useWorkflowThread";
import { ThreadItem } from "@/types/chat";
import { Workflow } from "lucide-react";
import { useEffect } from "react";
import ProcessingWarning from "../ProcessingWarning";

const WorkflowThread = ({
  thread,
  refetchThread,
}: {
  thread: ThreadItem;
  refetchThread: () => void;
}) => {
  const { setLogs } = useWorkflowThreadStore();
  const { logs, isLoading } = useWorkflowThreadStore(
    (state) =>
      state.workflowThread.get(thread.id) || { logs: [], isLoading: false },
  );

  useEffect(() => {
    if (thread.output && !isLoading) {
      setLogs(thread.id, () => JSON.parse(thread.output));
    }
  }, [thread, isLoading, setLogs]);

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
        <ProcessingWarning
          className="max-w-page-content mx-auto w-full mt-2"
          threadId={thread.id}
          isLoading={isLoading}
          onRefresh={refetchThread}
        />

        <OutputLogs
          isPending={isLoading}
          logs={logs}
          contentClassName="max-w-page-content mx-auto"
        />
      </div>
    </div>
  );
};

export default WorkflowThread;
