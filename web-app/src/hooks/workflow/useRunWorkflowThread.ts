/**
 * Runs a workflow inside a thread context — replaces the legacy
 * `WorkflowService.runWorkflowThread` path. Drives the new
 * `/agentic-workflows/runs` SSE stream and feeds the same `LogItem` tree
 * the run page uses into `useWorkflowThreadStore`.
 *
 * The tree is rebuilt from the accumulated event log on every SSE message
 * so the thread surface stays identical to the run-page surface — including
 * per-step nested SQL/results/agent content.
 */

import { useQueryClient } from "@tanstack/react-query";
import { useCallback } from "react";
import { toast } from "sonner";

import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { decodeBase64 } from "@/libs/encoding";
import { AgenticWorkflowService, type WorkflowEvent } from "@/services/api/agenticWorkflows";
import type { LogItem } from "@/services/types";
import useWorkflowThreadStore from "@/stores/useWorkflowThread";
import type { ThreadItem, ThreadsResponse } from "@/types/chat";
import { buildLogItems } from "../api/agentic-workflows/useLogItems";
import queryKeys from "../api/queryKey";

const useRunWorkflowThread = () => {
  const queryClient = useQueryClient();
  const { project } = useCurrentProjectBranch();
  const projectId = project.id;
  const { setLogs, setIsLoading, getWorkflowThread } = useWorkflowThreadStore();

  /**
   * Run the workflow whose path-base64 was stored as the thread's
   * `source` when the thread was created. The caller passes both ids
   * because the chat-panel callback already has them; we don't need a
   * round-trip GET /threads/:id to recover the source.
   */
  const run = useCallback(
    async (threadId: string, workflowSourceB64: string) => {
      const { isLoading } = getWorkflowThread(threadId);
      if (isLoading) return;

      const workflowRef = decodeBase64(workflowSourceB64);

      // Mark the thread as in-flight in the global thread cache so the
      // sidebar / list views show the spinner without a refetch.
      queryClient.setQueryData(
        queryKeys.thread.list(projectId, 1, 50),
        (old: ThreadsResponse | undefined) =>
          old
            ? {
                ...old,
                threads: old.threads.map((item) =>
                  item.id === threadId ? { ...item, is_processing: true } : item
                )
              }
            : old
      );
      setIsLoading(threadId, true);
      setLogs(threadId, () => []);

      // Per-run accumulator. Rebuilds the LogItem tree on each event so the
      // store's `setLogs(prev => next)` reducer stays trivial — the truth is
      // the event log, not the partial log items.
      const events: WorkflowEvent[] = [];
      const pushTree = () => {
        const tree = buildLogItems(events);
        setLogs(threadId, () => tree);
      };

      try {
        const { run_id } = await AgenticWorkflowService.startRun(projectId, {
          workflow_ref: workflowRef,
          // Link the run to the thread so a page reload can resume from
          // `agentic_runs.thread_id` instead of falling back to the now-
          // empty in-memory zustand log buffer.
          thread_id: threadId
        });
        await AgenticWorkflowService.streamEvents(projectId, run_id, {
          onEvent: (event) => {
            events.push(event);
            pushTree();
          }
        });
      } catch (error) {
        console.error("Error running workflow thread:", error);
        toast.error("An error occurred while running the workflow thread. Please try again.");
        const failure: LogItem = {
          timestamp: new Date().toISOString(),
          log_type: "error",
          content: `Workflow run failed: ${error instanceof Error ? error.message : String(error)}`
        };
        setLogs(threadId, (prev) => [...prev, failure]);
      } finally {
        queryClient.setQueryData(
          queryKeys.thread.item(projectId, threadId),
          (old: ThreadItem | undefined) => (old ? { ...old, is_processing: false } : old)
        );
        queryClient.invalidateQueries({ queryKey: queryKeys.thread.all });
        setIsLoading(threadId, false);
      }
    },
    [getWorkflowThread, projectId, queryClient, setIsLoading, setLogs]
  );

  return { run };
};

export default useRunWorkflowThread;
