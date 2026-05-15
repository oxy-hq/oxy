import { Workflow } from "lucide-react";
import { useMemo } from "react";
import PageHeader from "@/components/PageHeader";
import { Separator } from "@/components/ui/shadcn/separator";
import OutputLogs from "@/components/workflow/output/Logs";
import { useResumeWorkflowThread } from "@/hooks/workflow/useResumeWorkflowThread";
import { decodeBase64 } from "@/libs/encoding";
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
  const { workflowThread } = useWorkflowThreadStore();

  const { logs, isLoading } = workflowThread.get(thread.id) || {
    logs: [],
    isLoading: false
  };

  // Recover from `agentic_runs.thread_id` after a page reload: the
  // zustand store is in-memory, so the live runner's logs are gone after
  // a refresh. The resume hook fetches the latest workflow run for this
  // thread and replays its persisted events into the store. No-ops when
  // the store already has logs (active run in progress).
  useResumeWorkflowThread(thread.id);

  // `thread.source` is stored as URL-safe base64 of the workflow file
  // path (matches the workflow run page's URL param). Decode for the
  // header so the user sees the readable path; fall back to the raw
  // value if decoding fails so we never render an empty header.
  const sourcePath = useMemo(() => {
    const source = thread?.source;
    if (!source) return "";
    try {
      return decodeBase64(source);
    } catch {
      return source;
    }
  }, [thread?.source]);

  return (
    <div className='flex h-full flex-col'>
      <PageHeader className='items-center border-border border-b-1'>
        <div className='flex h-full flex-1 items-center justify-center p-2'>
          <div className='flex items-center gap-1 text-muted-foreground'>
            <Workflow className='h-4 min-h-4 w-4 min-w-4' />
            <p className='break-all text-sm'>{sourcePath}</p>
          </div>
          <div className='flex h-full items-stretch px-4'>
            <Separator orientation='vertical' />
          </div>

          <p className='text-base-foreground text-sm'>{thread?.title}</p>
        </div>
      </PageHeader>

      <div className='w-full flex-1'>
        <div className='px-4'>
          <ProcessingWarning
            className='mx-auto mt-2 w-full max-w-page-content'
            threadId={thread.id}
            isLoading={isLoading}
            onRefresh={refetchThread}
          />
        </div>

        <OutputLogs
          isPending={isLoading}
          logs={logs}
          contentClassName='max-w-page-content mx-auto mt-4'
        />
      </div>
    </div>
  );
};

export default WorkflowThread;
