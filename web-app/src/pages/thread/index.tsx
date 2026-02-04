import { useQueryClient } from "@tanstack/react-query";
import { AlertTriangle } from "lucide-react";
import { useParams } from "react-router-dom";
import PageSkeleton from "@/components/PageSkeleton";
import queryKeys from "@/hooks/api/queryKey";
import useThread from "@/hooks/api/threads/useThread";
import AgentThread from "./agent";
import AgenticThread from "./agentic";
import TaskThread from "./task";
import WorkflowThread from "./workflow";

const ThreadNotFound = () => (
  <div className='flex h-64 flex-col items-center justify-center p-8 text-center'>
    <AlertTriangle className='mb-4 h-16 w-16 text-amber-500' />
    <h2 className='mb-2 font-semibold text-2xl text-gray-700'>Thread Not Found</h2>
    <p className='max-w-md text-gray-500'>
      The thread you're looking for doesn't exist or may have been removed.
    </p>
    <button
      onClick={() => window.history.back()}
      className='mt-6 rounded-md bg-blue-600 px-4 py-2 text-white transition-colors hover:bg-blue-700'
    >
      Go Back
    </button>
  </div>
);

const Thread = () => {
  const { threadId } = useParams();
  const queryClient = useQueryClient();
  const { data: thread, isPending, isSuccess, refetch } = useThread(threadId ?? "", true, false);

  if (isPending) {
    return <PageSkeleton />;
  }

  if (!thread) {
    return <ThreadNotFound />;
  }

  const refetchThread = () => {
    queryClient.invalidateQueries({
      queryKey: queryKeys.thread.all
    });
    refetch();
  };

  if (isSuccess && thread) {
    switch (thread.source_type) {
      case "workflow":
        return <WorkflowThread thread={thread} refetchThread={refetchThread} />;
      case "agent":
        return <AgentThread thread={thread} refetchThread={refetchThread} />;
      case "task":
        return <TaskThread thread={thread} refetchThread={refetchThread} />;
      case "agentic":
        return <AgenticThread key={thread.id} thread={thread} />;
      default:
        return <AgentThread thread={thread} refetchThread={refetchThread} />;
    }
  }

  return <ThreadNotFound />;
};

const ThreadPage = () => {
  const { threadId } = useParams();
  return <Thread key={threadId} />;
};

export default ThreadPage;
