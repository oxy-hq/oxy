import { useQueryClient } from "@tanstack/react-query";
import queryKeys from "../api/queryKey";
import { ThreadItem, ThreadsResponse } from "@/types/chat";
import useWorkflowThreadStore from "@/stores/useWorkflowThread";
import { LogItem } from "@/services/types";
import { useCallback } from "react";
import throttle from "lodash/throttle";
import { WorkflowService } from "@/services/api";
import { toast } from "sonner";
import useCurrentProjectBranch from "../useCurrentProjectBranch";

const useRunWorkflowThread = () => {
  const queryClient = useQueryClient();
  const { project, branchName } = useCurrentProjectBranch();
  const projectId = project.id;
  const { setLogs, setIsLoading, getWorkflowThread } = useWorkflowThreadStore();

  const appendLogs = useCallback(
    (newLogs: LogItem[], threadId: string) => {
      setLogs(threadId, (pre) => [...pre, ...newLogs]);
    },
    [setLogs],
  );

  const processLogs = useCallback(
    (threadId: string) => {
      let buffer: LogItem[] = [];
      const flushLogs = throttle(
        () => {
          const logsToAppend = [...buffer];
          appendLogs(logsToAppend, threadId);
          buffer = [];
        },
        500,
        { leading: true, trailing: true },
      );

      return (logItem: LogItem) => {
        buffer.push(logItem);
        flushLogs();
      };
    },
    [appendLogs],
  );

  const run = async (threadId: string) => {
    const { isLoading } = getWorkflowThread(threadId);

    if (isLoading) return;

    queryClient.setQueryData(
      queryKeys.thread.list(projectId, 1, 50),
      (old: ThreadsResponse | undefined) => {
        if (old) {
          return {
            ...old,
            threads: old.threads.map((item) =>
              item.id === threadId ? { ...item, is_processing: true } : item,
            ),
          };
        }
        return old;
      },
    );

    setIsLoading(threadId, true);
    setLogs(threadId, () => []);

    WorkflowService.runWorkflowThread(
      projectId,
      branchName,
      threadId,
      processLogs(threadId),
    )
      .finally(() => {
        queryClient.setQueryData(
          queryKeys.thread.item(projectId, threadId),
          (old: ThreadItem | undefined) => {
            if (old) {
              return { ...old, is_processing: false };
            }
            return old;
          },
        );

        queryClient.invalidateQueries({
          queryKey: queryKeys.thread.all,
        });
        setIsLoading(threadId, false);
      })
      .catch((error) => {
        console.error("Error running workflow thread:", error);
        toast.error(
          "An error occurred while running the workflow thread. Please try again.",
        );
      });
  };

  return { run };
};

export default useRunWorkflowThread;
