import useThread from "@/hooks/api/useThread";
import { useParams } from "react-router-dom";
import WorkflowThread from "./workflow";
import AgentThread from "./agent";
import TaskThread from "./task";
import PageSkeleton from "@/components/PageSkeleton";
import { AlertTriangle } from "lucide-react";

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
  const { data: thread, isPending, isSuccess } = useThread(threadId ?? "");

  if (isPending) {
    return <PageSkeleton />;
  }

  if (!thread) {
    return <ThreadNotFound />;
  }

  if (isSuccess && thread) {
    switch (thread.source_type) {
      case "workflow":
        return <WorkflowThread thread={thread} />;
      case "agent":
        return <AgentThread thread={thread} />;
      case "task":
        return <TaskThread thread={thread} />;
      default:
        return <AgentThread thread={thread} />;
    }
  }

  return <ThreadNotFound />;
};

const ThreadPage = () => {
  const { threadId } = useParams();
  return <Thread key={threadId} />;
};

export default ThreadPage;
