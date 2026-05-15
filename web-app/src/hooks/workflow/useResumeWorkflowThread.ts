/**
 * Recover a chat-thread workflow's logs after a page reload.
 *
 * The active-run path keeps its `LogItem[]` in `useWorkflowThreadStore`
 * (zustand). That store is in-memory, so a refresh wipes it. Without
 * recovery the thread/workflow page renders an empty `OutputLogs` even
 * though the run actually completed and its events are on disk.
 *
 * This hook fires once per `threadId` mount: if the local zustand entry
 * is empty, it asks the backend for the latest workflow run linked to
 * that thread (`agentic_runs.thread_id`), opens an SSE connection to
 * replay every persisted event for that run, and rebuilds the LogItem
 * tree via the same `buildLogItems` aggregator the live runner uses.
 */

import { useEffect } from "react";

import { AgenticWorkflowService, type WorkflowEvent } from "@/services/api/agenticWorkflows";
import useWorkflowThreadStore from "@/stores/useWorkflowThread";
import { buildLogItems } from "../api/agentic-workflows/useLogItems";
import useCurrentProjectBranch from "../useCurrentProjectBranch";

export const useResumeWorkflowThread = (threadId: string | undefined) => {
  const { project } = useCurrentProjectBranch();
  const projectId = project.id;
  const { setLogs, setIsLoading, getWorkflowThread } = useWorkflowThreadStore();

  useEffect(() => {
    if (!threadId) return;
    // Don't disturb an active run: if the store already has logs for this
    // thread (because the user just kicked off the workflow from chat
    // and the live SSE is still streaming), the resume path would race
    // with it. The "is there anything to resume" check is local-state-
    // first, so a fresh run that hasn't emitted yet still gets resumed.
    const existing = getWorkflowThread(threadId);
    if (existing.logs.length > 0 || existing.isLoading) return;

    const abort = new AbortController();
    let cancelled = false;

    (async () => {
      const latest = await AgenticWorkflowService.latestRunForThread(projectId, threadId).catch(
        (e) => {
          console.error("resumeWorkflowThread: latest-run lookup failed", e);
          return null;
        }
      );
      if (cancelled || !latest) return;

      // The run row drives the loading indicator: if the backend still
      // shows it as in-flight (`running`/`delegating`/etc.), keep the
      // spinner up while we replay; once the SSE stream closes the
      // shared `is_terminal` path finalizes it.
      const isTerminal = ["done", "failed", "cancelled", "timed_out"].includes(
        latest.task_status ?? ""
      );
      setIsLoading(threadId, !isTerminal);

      const events: WorkflowEvent[] = [];
      const flush = () => {
        const tree = buildLogItems(events);
        setLogs(threadId, () => tree);
      };

      try {
        await AgenticWorkflowService.streamEvents(projectId, latest.run_id, {
          signal: abort.signal,
          onEvent: (event) => {
            events.push(event);
            flush();
          }
        });
      } catch (e) {
        if (!abort.signal.aborted) {
          console.error("resumeWorkflowThread: SSE replay failed", e);
        }
      } finally {
        if (!cancelled) setIsLoading(threadId, false);
      }
    })();

    return () => {
      cancelled = true;
      abort.abort();
    };
    // setLogs/setIsLoading/getWorkflowThread are stable zustand actions.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [threadId, projectId, setLogs, setIsLoading, getWorkflowThread]);
};
