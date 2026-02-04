import { Workflow } from "lucide-react";
import { useEffect } from "react";
import PageHeader from "@/components/PageHeader";
import { Separator } from "@/components/ui/shadcn/separator";
import OutputLogs from "@/components/workflow/output/Logs";
import useWorkflowThreadStore from "@/stores/useWorkflowThread";
import type { ThreadItem } from "@/types/chat";
import ProcessingWarning from "../ProcessingWarning";

const WorkflowThread = ({
  thread,
  refetchThread
}: {
  thread: ThreadItem;
  refetchThread: () => void;
}) => {
  const { setLogs, workflowThread } = useWorkflowThreadStore();

  const { logs, isLoading } = workflowThread.get(thread.id) || {
    logs: [],
    isLoading: false
  };

  useEffect(() => {
    if (thread.output && !isLoading) {
      setLogs(thread.id, () => JSON.parse(thread.output));
    }
  }, [thread, isLoading, setLogs]);

  return (
    <div className='flex h-full flex-col'>
      <PageHeader className='items-center border-border border-b-1'>
        <div className='flex h-full flex-1 items-center justify-center p-2'>
          <div className='flex items-center gap-1 text-muted-foreground'>
            <Workflow className='h-4 min-h-4 w-4 min-w-4' />
            <p className='break-all text-sm'>{thread?.source}</p>
          </div>
          <div className='flex h-full items-stretch px-4'>
            <Separator orientation='vertical' />
          </div>

          <p className='text-base-foreground text-sm'>{thread?.title}</p>
        </div>
      </PageHeader>

      <div className='w-full flex-1'>
        <ProcessingWarning
          className='mx-auto mt-2 w-full max-w-page-content'
          threadId={thread.id}
          isLoading={isLoading}
          onRefresh={refetchThread}
        />

        <OutputLogs
          isPending={isLoading}
          logs={logs}
          contentClassName='max-w-page-content mx-auto'
        />
      </div>
    </div>
  );
};

export default WorkflowThread;
