import { useBlockStore } from "./block";
import { RunService, ThreadService } from "@/services/api";
import useTaskThreadStore from "./useTaskThread";
import { useCallback, useEffect, useMemo } from "react";
import { useStreamEvents } from "@/components/workflow/useWorkflowRun";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { useMutation, useQuery } from "@tanstack/react-query";
import { MessageFactory } from "@/hooks/messaging/core/messageFactory";
import { Block, RunInfo } from "@/services/types";
import { Message } from "@/types/chat";
import queryKeys from "@/hooks/api/queryKey";

export const useAgenticStore = (projectId: string, threadId: string) => {
  const result = useThreadMessages(projectId, threadId);
  const { setMessages } = useTaskThreadStore();
  const { setGroupBlocks } = useBlockStore();

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
            message.run_info.metadata,
          );
        }
      });
    }
  }, [result.data, threadId]);

  return result;
};

export const useThreadMessages = (projectId: string, threadId: string) => {
  return useQuery({
    queryKey: queryKeys.thread.messages(projectId, threadId),
    queryFn: () => ThreadService.getThreadMessages(projectId, threadId),
  });
};

export const useObserveAgenticMessages = (
  threadId: string,
  refetch?: () => Promise<unknown>,
) => {
  const onGoingMessages = useMessages(threadId, usePendingPred());
  const setGroupBlocks = useBlockStore((state) => state.setGroupBlocks);
  const { stream } = useStreamEvents();

  useEffect(() => {
    const abortRef = new AbortController();
    const observeMessages = async () => {
      const message = onGoingMessages[onGoingMessages.length - 1];
      if (!message || !message.run_info) return;

      setGroupBlocks(
        message.run_info,
        {},
        [],
        undefined,
        message.run_info.metadata,
        true,
      );
      await stream
        .mutateAsync({
          sourceId: message.run_info.source_id,
          runIndex: message.run_info.run_index,
          abortRef: abortRef.signal,
        })
        .catch((error) => {
          console.error(
            "Failed to observe agentic message stream:",
            Object.keys(error),
          );
        })
        .finally(() => {
          refetch?.();
        });
    };
    observeMessages();

    return () => {
      abortRef.abort();
    };
  }, [threadId, onGoingMessages]);
};

export const useAskAgentic = () => {
  const { project, branchName } = useCurrentProjectBranch();
  const setGroupBlocks = useBlockStore((state) => state.setGroupBlocks);
  const { mergeMessages } = useTaskThreadStore();
  return useMutation({
    mutationFn: async ({
      prompt,
      threadId,
      agentRef,
    }: {
      prompt: string;
      threadId: string;
      agentRef: string;
    }) => {
      return await RunService.createAgenticRun(project.id, branchName, {
        threadId,
        prompt,
        agentRef,
      });
    },
    onMutate({ threadId, prompt }) {
      mergeMessages(threadId, [
        MessageFactory.createUserMessage(prompt, threadId),
      ]);
    },
    onSuccess({ message_id, run_info }, { threadId }) {
      mergeMessages(threadId, [
        MessageFactory.createAgenticMessage(message_id, threadId, run_info),
      ]);
      setGroupBlocks(run_info, {}, [], undefined, run_info.metadata);
    },
  });
};

export const useSelectedMessageReasoning = () => {
  const selectedGroupId = useBlockStore((state) => state.selectedGroupId);
  const setSelectedGroupId = useBlockStore((state) => state.setSelectedGroupId);
  const groupBlocks = useBlockStore((state) => state.groupBlocks);

  const group = groupBlocks[selectedGroupId || ""];
  const blocks = group?.root.map((childId) => group.blocks[childId]) || [];
  const reasoningSteps = blocks
    .filter((block) => block.type === "step")
    .map((stepBlock) => {
      const childBlocks = stepBlock.children.flatMap((childId) => {
        const childBlock = group.blocks[childId];
        return blockTraverse(childBlock, group.blocks, isRenderableBlock);
      });
      return {
        ...stepBlock,
        childrenBlocks: childBlocks,
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
    [setSelectedGroupId],
  );

  const selectBlock = useCallback(
    (blockId: string, runInfo?: RunInfo) => {
      if (!runInfo) return false;
      const groupId = getGroupId(runInfo);
      setSelectedGroupId(groupId);
      setSelectedBlockId(blockId);
    },
    [setSelectedGroupId, setSelectedBlockId],
  );

  return {
    selectedBlock,
    selectedGroupId,
    reasoningSteps,
    selectReasoning,
    selectBlock,
    setSelectedGroupId,
    setSelectedBlockId,
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
        (block) => block.type === "data_app" && block.file_path,
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
  if ((children && children.length == 0) || !blocks) {
    return null;
  }
  return children
    .map((childrenId) => blocks[childrenId])
    .filter(
      (block) =>
        block.type === "step" && ["end", "build_app"].includes(block.step_type),
    )
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
          .filter((message) => !!message.run_info)
          .map((message) =>
            RunService.cancelRun(
              project.id,
              branchName,
              message.run_info!.source_id,
              message.run_info!.run_index,
            ),
          ),
      );
    },
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
      return (
        !!message.run_info && !!processingGroups[getGroupId(message.run_info)]
      );
    },
    [processingGroups],
  );
};

const useAllPred = () => {
  return useCallback((message: Message) => {
    return !!message.run_info;
  }, []);
};

const usePendingPred = () => {
  return useCallback((message: Message) => {
    return (
      !!message.run_info &&
      ["pending", "running"].includes(message.run_info.status)
    );
  }, []);
};

const isArtifactBlock = (block: Block) => {
  return ["data_app", "sql", "viz"].includes(block.type);
};

const isRenderableBlock = (block: Block) => {
  return isArtifactBlock(block) || block.type === "text";
};

const getGroupId = (runInfo?: RunInfo) => {
  if (!runInfo) return "";
  return `${runInfo.source_id}::${runInfo.run_index}`;
};

function filterMapBlock<T = Block>(
  runInfo: RunInfo,
  groupBlocks: Record<
    string,
    { blocks: Record<string, Block>; root: string[] }
  >,
  predicate: (block: Block) => boolean,
  map: (block: Block) => T = (b) => b as unknown as T,
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
      result = [
        ...result,
        ...blockTraverse(childBlock, group.blocks, predicate, map),
      ];
    }
  }

  return result;
}

function blockTraverse<T = Block>(
  block: Block,
  blocks: Record<string, Block>,
  predicate: (block: Block) => boolean,
  map: (block: Block) => T = (b) => b as unknown as T,
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
        result = [
          ...result,
          ...blockTraverse(childBlock, blocks, predicate, map),
        ];
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
      if (block && block.children) {
        for (const childId of block.children) {
          if (
            (!visited.has(childId) && dfs(childId)) ||
            recStack.has(childId)
          ) {
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
