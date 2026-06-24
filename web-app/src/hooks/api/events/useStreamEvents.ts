/**
 * SSE block-event streamer for the legacy run pipeline.
 *
 * Drives the analytics chat thread's reasoning trace: the
 * `useObserveAgenticMessages` hook in `stores/agentic.ts` calls
 * `stream.mutateAsync` whenever a thread has an in-flight run, and
 * `useBlockStore.handleEvent` folds each frame into the per-group
 * block tree.
 *
 * Why it lives outside the automation tree now: the automation run page
 * uses the new `/agentic-automations/runs/:id/events` SSE (typed
 * `AutomationEvent`s, handled by `useAutomationRunStream`). The legacy
 * `/events` SSE this hook subscribes to is still load-bearing for
 * analytics chat blocks — keeping it here makes the dependency
 * direction honest (analytics chat depends on a generic event stream
 * hook, not on a automation component).
 */

import { useMutation } from "@tanstack/react-query";

import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { RunService } from "@/services/api";
import type { BlockEvent } from "@/services/types";
import { useBlockStore } from "@/stores/block";

const getGroupId = (sourceId: string, runId?: string): string =>
  runId ? `${sourceId}::${runId}` : sourceId;

export const useStreamEvents = () => {
  const { project, branchName } = useCurrentProjectBranch();
  const handleEvent = useBlockStore((state) => state.handleEvent);
  const cleanupGroupStacks = useBlockStore((state) => state.cleanupGroupStacks);
  const setGroupProcessing = useBlockStore((state) => state.setGroupProcessing);
  const mutation = useMutation({
    mutationFn: async ({
      sourceId,
      runIndex,
      abortRef
    }: {
      sourceId: string;
      runIndex: number;
      abortRef?: AbortSignal;
    }) => {
      // Batch SSE events per animation frame to reduce re-renders.
      let eventQueue: BlockEvent[] = [];
      let rafId: number | null = null;

      const flushEvents = () => {
        rafId = null;
        const batch = eventQueue;
        eventQueue = [];
        for (const event of batch) {
          handleEvent(event);
        }
      };

      const batchedHandler = (event: BlockEvent) => {
        eventQueue.push(event);
        if (rafId === null) {
          rafId = requestAnimationFrame(flushEvents);
        }
      };

      return await RunService.streamEvents(
        project.id,
        branchName,
        { sourceId, runIndex },
        batchedHandler,
        () => {
          // Flush any remaining events before cleanup.
          if (rafId !== null) {
            cancelAnimationFrame(rafId);
            rafId = null;
          }
          for (const event of eventQueue) {
            handleEvent(event);
          }
          eventQueue = [];
          cleanupGroupStacks("Cancelled");
          const groupId = getGroupId(sourceId, runIndex.toString());
          setGroupProcessing(groupId, false);
        },
        (error) => {
          if (rafId !== null) {
            cancelAnimationFrame(rafId);
          }
          console.error("Stream error:", error);
          const groupId = getGroupId(sourceId, runIndex.toString());
          setGroupProcessing(groupId, false);
        },
        abortRef
      );
    },
    onMutate: ({ sourceId, runIndex }) => {
      const groupId = getGroupId(sourceId, runIndex.toString());
      setGroupProcessing(groupId, true);
    },
    onError: (_error, { sourceId, runIndex }) => {
      const groupId = getGroupId(sourceId, runIndex.toString());
      setGroupProcessing(groupId, false);
    }
  });

  return { stream: mutation };
};
