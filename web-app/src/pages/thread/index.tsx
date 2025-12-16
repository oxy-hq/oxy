import useThread from "@/hooks/api/threads/useThread";
import { useParams } from "react-router-dom";
import WorkflowThread from "./workflow";
import AgentThread from "./agent";
import TaskThread from "./task";
import PageSkeleton from "@/components/PageSkeleton";
import { AlertTriangle } from "lucide-react";
import { useQueryClient } from "@tanstack/react-query";
import queryKeys from "@/hooks/api/queryKey";
import AgenticThread from "./agentic";

const ThreadNotFound = () => (
  <div className="flex flex-col items-center justify-center h-64 p-8 text-center">
    <AlertTriangle className="w-16 h-16 text-amber-500 mb-4" />
    <h2 className="text-2xl font-semibold text-gray-700 mb-2">
      Thread Not Found
    </h2>
    <p className="text-gray-500 max-w-md">
      The thread you're looking for doesn't exist or may have been removed.
    </p>
    <button
      onClick={() => window.history.back()}
      className="mt-6 px-4 py-2 bg-blue-600 text-white rounded-md hover:bg-blue-700 transition-colors"
    >
      Go Back
    </button>
  </div>
);

const Thread = () => {
  const { threadId } = useParams();
  const queryClient = useQueryClient();
  const {
    data: thread,
    isPending,
    isSuccess,
    refetch,
  } = useThread(threadId ?? "", true, false);

  if (isPending) {
    return <PageSkeleton />;
  }

  if (!thread) {
    return <ThreadNotFound />;
  }

  const refetchThread = () => {
    queryClient.invalidateQueries({
      queryKey: queryKeys.thread.all,
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
