import useThread from "@/hooks/api/useThread";
import { useParams } from "react-router-dom";
import WorkflowThread from "./workflow";
import AgentThread from "./agent";
import TaskThread from "./task";

const Thread = () => {
  const { threadId } = useParams();
  const { data: thread, isLoading, isSuccess } = useThread(threadId ?? "");

  if (isLoading) {
    return <div>Loading...</div>;
  }

  if (!thread) {
    return <div>Thread not found</div>;
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

  return <div>Thread not found</div>;
};

const ThreadPage = () => {
  const { threadId } = useParams();
  return <Thread key={threadId} />;
};

export default ThreadPage;
