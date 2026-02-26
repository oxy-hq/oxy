import { useMutation, useQuery } from "@tanstack/react-query";
import { useCallback, useEffect, useMemo } from "react";
import { useStreamEvents } from "@/components/workflow/useWorkflowRun";
import queryKeys from "@/hooks/api/queryKey";
import { MessageFactory } from "@/hooks/messaging/core/messageFactory";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import type { Step } from "@/pages/thread/agentic/ArtifactSidebar/ArtifactBlockRenderer/SubGroupReasoningPanel/Reasoning";
import { RunService, ThreadService } from "@/services/api";
import type {
  Block,
  BlockBase,
  RunInfo,
  StepContent,
  StepType,
  TaskContent
} from "@/services/types";
import type { Message } from "@/types/chat";
import { useBlockStore } from "./block";
import useTaskThreadStore from "./useTaskThread";

export const useAgenticStore = (projectId: string, threadId: string) => {
  const result = useThreadMessages(projectId, threadId);
  const { setMessages } = useTaskThreadStore();
  const { setGroupBlocks } = useBlockStore();
  const { project, branchName } = useCurrentProjectBranch();

  // biome-ignore lint/correctness/useExhaustiveDependencies: setMessages, setGroupBlocks, project, and branchName are stable references that don't need to trigger re-runs
  useEffect(() => {
    const messages = result.data;
    if (messages) {
      setMessages(threadId, messages);
      // Set group blocks for messages that have runs
      messages.forEach((message) => {
        if (!!message.run_info && message.run_info.source_id) {
          setGroupBlocks(
            message.run_info,
            message.run_info.blocks,
            message.run_info.children,
            message.run_info.error,
            message.run_info.metadata
          );

          // Eagerly fetch nested sub-group blocks (e.g., routed automations)
          if (message.run_info.blocks) {
            for (const block of Object.values(message.run_info.blocks)) {
              if (block.type !== "group") continue;
              const [sourceId, runIndexStr] = block.group_id.split("::");
              const runIndex = runIndexStr != null ? parseInt(runIndexStr, 10) : undefined;
              RunService.getBlocks(project.id, branchName, {
                source_id: sourceId,
                run_index: runIndex
              })
                .then((groups) => {
                  for (const group of groups) {
                    setGroupBlocks(
                      group,
                      group.blocks,
                      group.children,
                      group.error,
                      group.metadata
                    );
                  }
                })
                .catch(() => {
                  console.error(`Failed to fetch blocks for group ${block.group_id}`);
                });
            }
          }
        }
      });
    }
  }, [result.data, threadId]);

  return result;
};

export const useThreadMessages = (projectId: string, threadId: string) => {
  return useQuery({
    queryKey: queryKeys.thread.messages(projectId, threadId),
    queryFn: () => ThreadService.getThreadMessages(projectId, threadId)
  });
};

export const useObserveAgenticMessages = (threadId: string, refetch?: () => Promise<unknown>) => {
  const onGoingMessages = useMessages(threadId, usePendingPred());
  const setGroupBlocks = useBlockStore((state) => state.setGroupBlocks);
  const { stream } = useStreamEvents();

  // Derive stable primitive values from the last pending message so the effect
  // only re-fires when the actual stream identity changes, not when the
  // onGoingMessages array reference changes due to unrelated store updates.
  const lastMessage = onGoingMessages[onGoingMessages.length - 1];
  const streamSourceId = lastMessage?.run_info?.source_id ?? "";
  const streamRunIndex = lastMessage?.run_info?.run_index ?? -1;
  const streamMetadata = lastMessage?.run_info?.metadata;

  // biome-ignore lint/correctness/useExhaustiveDependencies: deps are primitive sourceId/runIndex to prevent unnecessary stream restarts
  useEffect(() => {
    if (!streamSourceId || streamRunIndex < 0 || !lastMessage?.run_info) return;
    const abortRef = new AbortController();

    setGroupBlocks(lastMessage.run_info, {}, [], undefined, streamMetadata, true);
    stream
      .mutateAsync({
        sourceId: streamSourceId,
        runIndex: streamRunIndex,
        abortRef: abortRef.signal
      })
      .catch((error) => {
        console.error("Failed to observe agentic message stream:", Object.keys(error));
      })
      .finally(() => {
        refetch?.();
      });

    return () => {
      abortRef.abort();
    };
  }, [threadId, streamSourceId, streamRunIndex]);
};

export const useAskAgentic = () => {
  const { project, branchName } = useCurrentProjectBranch();
  const setGroupBlocks = useBlockStore((state) => state.setGroupBlocks);
  const { mergeMessages } = useTaskThreadStore();
  return useMutation({
    mutationFn: async ({
      prompt,
      threadId,
      agentRef
    }: {
      prompt: string;
      threadId: string;
      agentRef: string;
    }) => {
      return await RunService.createAgenticRun(project.id, branchName, {
        threadId,
        prompt,
        agentRef
      });
    },
    onMutate({ threadId, prompt }) {
      mergeMessages(threadId, [MessageFactory.createUserMessage(prompt, threadId)]);
    },
    onSuccess({ message_id, run_info }, { threadId }) {
      mergeMessages(threadId, [
        MessageFactory.createAgenticMessage(message_id, threadId, run_info)
      ]);
      setGroupBlocks(run_info, {}, [], undefined, run_info.metadata);
    }
  });
};

const getReasoningStepsFromGroupBlocks = (
  groupBlocks: Record<string, { blocks: Record<string, Block>; root: string[] }>,
  runInfo?: RunInfo
) => {
  if (!runInfo) return [];
  const group = groupBlocks[getGroupId(runInfo)];
  if (!group) return [];

  return group.root
    .map((childId) => group.blocks[childId])
    .filter(
      (block): block is BlockBase & StepContent =>
        block.type === "step" && block.step_type !== "end"
    )
    .map((stepBlock) => {
      const routeGroupChild =
        stepBlock.step_type === "route"
          ? (stepBlock.children
              .map((childId) => group.blocks[childId])
              .find((child) => child?.type === "group") ??
            // Fallback: group block may be at root level instead of nested under the step
            Object.values(group.blocks).find((b) => b.type === "group"))
          : undefined;
      const allChildren = stepBlock.children.flatMap((childId) => {
        const child = group.blocks[childId];
        return child ? blockTraverse(child, group.blocks, isRenderableBlock) : [];
      });
      // Extract route name before filtering the text block out of rendered children
      let routeName: string | undefined;
      if (stepBlock.step_type === "route") {
        const routeText = allChildren.find(
          (c) => c.type === "text" && c.content?.startsWith("Selected route:")
        );
        if (routeText?.type === "text") {
          routeName = routeText.content.match(/Selected route:\s*\*{0,2}(.+?)\*{0,2}\s*$/)?.[1];
        }
      }
      const childrenBlocks =
        stepBlock.step_type === "route"
          ? allChildren.filter(
              (child) => !(child.type === "text" && child.content?.startsWith("Selected route:"))
            )
          : allChildren;
      return {
        ...stepBlock,
        childrenBlocks,
        ...(routeGroupChild?.type === "group" ? { routeGroupId: routeGroupChild.group_id } : {}),
        ...(routeName ? { routeName } : {})
      };
    });
};

export const getMessageReasoningSteps = (runInfo?: RunInfo) => {
  const { groupBlocks } = useBlockStore.getState();
  return getReasoningStepsFromGroupBlocks(groupBlocks, runInfo);
};

export const useMessageReasoningSteps = (runInfo?: RunInfo) => {
  const groupBlocks = useBlockStore((state) => state.groupBlocks);
  return getReasoningStepsFromGroupBlocks(groupBlocks, runInfo);
};

export const useGroupReasoningSteps = (groupId: string | null) => {
  const groupBlocks = useBlockStore((state) => state.groupBlocks);
  if (!groupId) return [];
  const group = groupBlocks[groupId];
  if (!group) return [];

  const rootBlocks = group.root.map((childId) => group.blocks[childId]).filter(Boolean);
  const stepOrTaskBlocks = rootBlocks.filter(
    (block) => block.type === "step" || block.type === "task"
  );

  // If no step/task blocks, follow nested group chain (e.g., artifact wrapping workflow)
  if (stepOrTaskBlocks.length === 0) {
    const nestedGroupBlock = rootBlocks.find((block) => block.type === "group");
    if (nestedGroupBlock?.type === "group") {
      const nestedGroup = groupBlocks[nestedGroupBlock.group_id];
      if (nestedGroup) {
        return mapReasoningSteps(nestedGroup);
      }
    }
    return [];
  }

  return mapReasoningSteps(group);
};

export const useGroupStreaming = (groupId: string | null) => {
  const processingGroups = useBlockStore((state) => state.processingGroups);
  if (!groupId) return false;
  return !!processingGroups[groupId];
};

export const useSelectedMessageReasoning = () => {
  const selectedGroupId = useBlockStore((state) => state.selectedGroupId);
  const setSelectedGroupId = useBlockStore((state) => state.setSelectedGroupId);
  const groupBlocks = useBlockStore((state) => state.groupBlocks);

  const group = groupBlocks[selectedGroupId || ""];
  const blocks = group?.root.map((childId) => group.blocks[childId]) || [];
  const reasoningSteps = blocks
    .filter((block): block is StepBlock => block.type === "step" && block.step_type !== "end")
    .map((stepBlock) => {
      const childBlocks = stepBlock.children.flatMap((childId) => {
        const childBlock = group.blocks[childId];
        return blockTraverse(childBlock, group.blocks, isRenderableBlock);
      });
      return {
        ...stepBlock,
        childrenBlocks: childBlocks
      };
    });

  const selectedBlockId = useBlockStore((state) => state.selectedBlockId);
  const setSelectedBlockId = useBlockStore((state) => state.setSelectedBlockId);
  const selectedBlock = group?.blocks[selectedBlockId || ""];

  const selectReasoning = useCallback(
    (runInfo?: RunInfo) => {
      if (!runInfo) return false;
      const id = getGroupId(runInfo);
      setSelectedGroupId(id);
    },
    [setSelectedGroupId]
  );

  const selectBlock = useCallback(
    (blockId: string, runInfo?: RunInfo) => {
      if (!runInfo) return false;
      const groupId = getGroupId(runInfo);
      setSelectedGroupId(groupId);
      setSelectedBlockId(blockId);
    },
    [setSelectedGroupId, setSelectedBlockId]
  );

  return {
    selectedBlock,
    selectedGroupId,
    reasoningSteps,
    selectReasoning,
    selectBlock,
    setSelectedGroupId,
    setSelectedBlockId
  };
};

export const useThreadDataApp = (threadId: string) => {
  const { groupBlocks } = useBlockStore();
  const { getTaskThread } = useTaskThreadStore();
  const { messages } = getTaskThread(threadId);
  const apps = messages.flatMap((message) => {
    if (message.run_info) {
      return filterMapBlock(
        message.run_info,
        groupBlocks,
        (block) => block.type === "data_app",
        (block) => block.type === "data_app" && block.file_path
      );
    }
    return [];
  });
  return apps[apps.length - 1];
};

export const useThreadArtifacts = (threadId: string) => {
  const { groupBlocks } = useBlockStore();
  const { getTaskThread } = useTaskThreadStore();
  const taskThread = getTaskThread(threadId);
  const { messages } = taskThread;
  const artifacts = messages.flatMap((message) => {
    if (message.run_info) {
      return filterMapBlock(message.run_info, groupBlocks, isArtifactBlock);
    }
    return [];
  });
  return artifacts;
};

export const useMessageContent = (runInfo?: RunInfo) => {
  const { groupBlocks } = useBlockStore();

  if (!runInfo) {
    return null;
  }

  const { blocks, root: children } = groupBlocks[getGroupId(runInfo)] || {};
  if ((children && children.length === 0) || !blocks) {
    return null;
  }
  return children
    .map((childrenId) => blocks[childrenId])
    .filter((block) => block.type === "step" && ["end", "build_app"].includes(block.step_type))
    .flatMap((block) => blockTraverse(block, blocks, isRenderableBlock));
};

export const useMessageStreaming = (runInfo?: RunInfo) => {
  const processingGroups = useBlockStore((state) => state.processingGroups);

  if (!runInfo) {
    return false;
  }

  return !!processingGroups[getGroupId(runInfo)];
};

export const useIsThreadLoading = (threadId: string) => {
  const messages = useMessages(threadId, useOnGoingPred());
  return messages.length > 0;
};

export const useLastStreamingMessage = (threadId: string) => {
  const messages = useMessages(threadId, useOnGoingPred());
  const lastRunInfo = messages[messages.length - 1]?.run_info;
  return useMessageContent(lastRunInfo); // Ensure content is loaded
};

export const useLastRunInfoGroupId = (threadId: string) => {
  const messages = useMessages(threadId, useAllPred());
  return getGroupId(messages[messages.length - 1]?.run_info);
};

export const useStopAgenticRun = (threadId: string) => {
  const { project, branchName } = useCurrentProjectBranch();
  const messages = useMessages(threadId, useOnGoingPred());
  const stopMutation = useMutation({
    mutationFn: async () => {
      return Promise.all(
        messages
          .filter(
            (
              message
            ): message is typeof message & { run_info: NonNullable<typeof message.run_info> } =>
              !!message.run_info
          )
          .map((message) =>
            RunService.cancelRun(
              project.id,
              branchName,
              message.run_info.source_id,
              message.run_info.run_index
            )
          )
      );
    }
  });

  return stopMutation;
};

const useMessages = (threadId: string, pred: (message: Message) => boolean) => {
  const getTaskThread = useTaskThreadStore((state) => state.getTaskThread);
  const taskThread = getTaskThread(threadId);
  const { messages } = taskThread;
  const filteredMessages = useMemo(() => {
    return messages.filter(pred);
  }, [messages, pred]);
  return filteredMessages;
};

const useOnGoingPred = () => {
  const processingGroups = useBlockStore((state) => state.processingGroups);
  return useCallback(
    (message: Message) => {
      return !!message.run_info && !!processingGroups[getGroupId(message.run_info)];
    },
    [processingGroups]
  );
};

const useAllPred = () => {
  return useCallback((message: Message) => {
    return !!message.run_info;
  }, []);
};

const PENDING_STATUSES = ["pending", "running"];
const usePendingPred = () => {
  return useCallback((message: Message) => {
    return !!message.run_info && PENDING_STATUSES.includes(message.run_info.status);
  }, []);
};

type StepBlock = BlockBase & StepContent;
type TaskBlock = BlockBase & TaskContent;

const mapReasoningSteps = (group: { blocks: Record<string, Block>; root: string[] }): Step[] => {
  return group.root
    .map((childId) => group.blocks[childId])
    .filter(
      (block): block is StepBlock | TaskBlock =>
        (block?.type === "step" && block.step_type !== "end") || block?.type === "task"
    )
    .map((block): Step => {
      const childrenBlocks = block.children.flatMap((childId) => {
        const child = group.blocks[childId];
        return child ? blockTraverse(child, group.blocks, isRenderableBlock) : [];
      });
      if (block.type === "task") {
        return {
          id: block.id,
          type: "step" as const,
          step_type: inferStepType(block.children, group.blocks),
          objective: block.task_name,
          children: block.children,
          error: block.error,
          is_streaming: block.is_streaming,
          childrenBlocks
        };
      }
      return { ...block, childrenBlocks };
    });
};

function inferStepType(children: string[], blocks: Record<string, Block>): StepType {
  for (const childId of children) {
    const child = blocks[childId];
    if (!child) continue;
    switch (child.type) {
      case "semantic_query":
        return "semantic_query";
      case "sql":
        return "query";
      case "viz":
        return "visualize";
      case "data_app":
        return "build_app";
      case "text":
        return "insight";
    }
    if ((child.type === "task" || child.type === "step") && child.children.length > 0) {
      const nested = inferStepType(child.children, blocks);
      if (nested !== "subflow") return nested;
    }
  }
  return "subflow";
}

const isArtifactBlock = (block: Block) => {
  return ["data_app", "sql", "viz"].includes(block.type);
};

const isRenderableBlock = (block: Block) => {
  return isArtifactBlock(block) || block.type === "text" || block.type === "semantic_query";
};

const getGroupId = (runInfo?: RunInfo) => {
  if (!runInfo) return "";
  return `${runInfo.source_id}::${runInfo.run_index}`;
};

function filterMapBlock<T = Block>(
  runInfo: RunInfo,
  groupBlocks: Record<string, { blocks: Record<string, Block>; root: string[] }>,
  predicate: (block: Block) => boolean,
  map: (block: Block) => T = (b) => b as unknown as T
): T[] {
  const groupId = getGroupId(runInfo);
  const group = groupBlocks[groupId];
  if (!group) {
    return [];
  }

  let result: T[] = [];
  for (const childId of group.root) {
    const childBlock = group.blocks[childId];
    if (childBlock) {
      result = [...result, ...blockTraverse(childBlock, group.blocks, predicate, map)];
    }
  }

  return result;
}

function blockTraverse<T = Block>(
  block: Block,
  blocks: Record<string, Block>,
  predicate: (block: Block) => boolean,
  map: (block: Block) => T = (b) => b as unknown as T
): T[] {
  let result: T[] = [];
  const circularBlocks = detectCircularBlock(blocks);
  if (circularBlocks) {
    return result;
  }
  if (predicate(block)) {
    result.push(map(block));
  }

  if (block.children && block.children.length > 0) {
    for (const childId of block.children) {
      const childBlock = blocks[childId];
      if (childBlock) {
        result = [...result, ...blockTraverse(childBlock, blocks, predicate, map)];
      }
    }
  }

  return result;
}

function detectCircularBlock(blocks: Record<string, Block>): Block[] | null {
  const visited: Set<string> = new Set();
  const recStack: Set<string> = new Set();
  const circularBlocks: Block[] = [];

  const dfs = (blockId: string): boolean => {
    if (!visited.has(blockId)) {
      visited.add(blockId);
      recStack.add(blockId);

      const block = blocks[blockId];
      if (block?.children) {
        for (const childId of block.children) {
          if ((!visited.has(childId) && dfs(childId)) || recStack.has(childId)) {
            circularBlocks.push(block);
            return true;
          }
        }
      }
    }
    recStack.delete(blockId);
    return false;
  };

  for (const blockId in blocks) {
    if (dfs(blockId)) {
      return circularBlocks;
    }
  }

  return null;
}
