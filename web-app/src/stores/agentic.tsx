import { useBlockStore } from "./block";
import { RunService, ThreadService } from "@/services/api";
import useTaskThreadStore from "./useTaskThread";
import {
  Fragment,
  ReactNode,
  useCallback,
  useEffect,
  useMemo,
  useState,
} from "react";
import { useStreamEvents } from "@/components/workflow/useWorkflowRun";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { useMutation, useQuery } from "@tanstack/react-query";
import { MessageFactory } from "@/hooks/messaging/core/messageFactory";
import { Block, RunInfo } from "@/services/types";
import TableVirtualized from "@/components/Markdown/components/TableVirtualized";
import Markdown from "@/components/Markdown";
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
  }, [result.data]);

  return result;
};

export const useThreadMessages = (projectId: string, threadId: string) => {
  return useQuery({
    queryKey: queryKeys.thread.messages(projectId, threadId),
    queryFn: () => ThreadService.getThreadMessages(projectId, threadId),
  });
};

export const useObserveAgenticMessages = (threadId: string) => {
  const onGoingMessages = useMessages(threadId, usePendingPred());
  const { stream, cancel } = useStreamEvents();

  useEffect(() => {
    const observeMessages = async () => {
      for (const message of onGoingMessages) {
        if (!message.run_info) return;

        await stream
          .mutateAsync({
            sourceId: message.run_info.source_id,
            runIndex: message.run_info.run_index,
          })
          .catch((error) => {
            console.error("Failed to observe agentic message stream:", error);
          });
      }
    };
    observeMessages();
  }, [threadId, onGoingMessages]);

  return cancel;
};

export const useAskAgentic = () => {
  const { project, branchName } = useCurrentProjectBranch();
  const setGroupBlocks = useBlockStore((state) => state.setGroupBlocks);
  const { setMessages, getTaskThread } = useTaskThreadStore();
  return useMutation({
    mutationFn: async ({
      prompt,
      threadId,
    }: {
      prompt: string;
      threadId: string;
    }) => {
      return await RunService.createAgenticRun(project.id, branchName, {
        threadId,
        prompt,
      });
    },
    onMutate({ threadId, prompt }) {
      const { messages } = getTaskThread(threadId);
      setMessages(threadId, [
        ...messages,
        MessageFactory.createUserMessage(prompt, threadId),
      ]);
    },
    onSuccess({ message_id, run_info }, { threadId }) {
      const { messages } = getTaskThread(threadId);
      setMessages(threadId, [
        ...messages,
        MessageFactory.createAgenticMessage(message_id, threadId, run_info),
      ]);
      setGroupBlocks(run_info, {}, [], undefined, run_info.metadata);
    },
  });
};

export const useSelectedMessageReasoning = () => {
  const [groupId, setGroupId] = useState<string | null>(null);
  const groupBlocks = useBlockStore((state) => state.groupBlocks);
  const group = groupBlocks[groupId || ""];
  const blocks = group?.root.map((childId) => group.blocks[childId]) || [];
  const reasoningSteps = blocks
    .filter((block) => block.type === "step")
    .map((stepBlock) => {
      const childBlocks = stepBlock.children.map((childId) => {
        const childBlock = group.blocks[childId];
        return {
          id: childBlock.id,
          content: blockContentTraverse(childBlock, group.blocks),
        };
      });
      return {
        ...stepBlock,
        childrenBlocks: childBlocks,
      };
    });

  const openReasoning = useCallback((runInfo?: RunInfo) => {
    if (!runInfo) return;
    setGroupId(getGroupId(runInfo));
  }, []);

  const closeReasoning = useCallback(() => {
    setGroupId(null);
  }, []);

  const toggleReasoning = useCallback(
    (runInfo?: RunInfo, isReasoningSelected?: boolean) => {
      if (!runInfo) return false;
      const id = getGroupId(runInfo);
      if (groupId === id && isReasoningSelected) {
        setGroupId(null);
        return false;
      } else {
        setGroupId(id);
        return true;
      }
    },
    [groupId],
  );

  return {
    groupId,
    reasoningSteps,
    openReasoning,
    closeReasoning,
    toggleReasoning,
  };
};

export const useThreadDataApp = (threadId: string) => {
  const { groupBlocks } = useBlockStore();
  const { getTaskThread } = useTaskThreadStore();
  const { messages } = getTaskThread(threadId);
  const apps = messages.flatMap((message) => {
    if (message.run_info) {
      return flattenMessageApps(groupBlocks, message.run_info);
    }
    return [];
  });
  return apps[apps.length - 1];
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
    .flatMap((block) => blockContentTraverse(block, blocks));
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

const usePendingPred = () => {
  return useCallback((message: Message) => {
    return (
      !!message.run_info &&
      ["pending", "running"].includes(message.run_info.status)
    );
  }, []);
};

const getGroupId = (runInfo: RunInfo) => {
  return `${runInfo.source_id}::${runInfo.run_index}`;
};

const flattenMessageApps = (
  groupBlocks: Record<
    string,
    { blocks: Record<string, Block>; root: string[] }
  >,
  runInfo: RunInfo,
) => {
  const { blocks, root: children } = groupBlocks[getGroupId(runInfo)] || {};
  if ((children && children.length == 0) || !blocks) {
    return [];
  }
  return children
    .map((childrenId) => blocks[childrenId])
    .filter((block) => block.type === "step" && block.step_type === "build_app")
    .flatMap((block) => dataAppBlockTraverse(block, blocks));
};

const dataAppBlockTraverse = (
  block: Block,
  blocks: Record<string, Block>,
): string[] => {
  let content = [];
  if (block.type === "data_app" && block.file_path) {
    content.push(block.file_path);
  }

  if (block.children && block.children.length > 0) {
    for (const childId of block.children) {
      const childBlock = blocks[childId];
      if (childBlock) {
        content = [...content, ...dataAppBlockTraverse(childBlock, blocks)];
      }
    }
  }
  return content;
};

const blockContentTraverse = (
  block: Block,
  blocks: Record<string, Block>,
): ReactNode[] => {
  let content = [];
  if (block.type === "text") {
    content.push(<BlockMarkdown key={block.id}>{block.content}</BlockMarkdown>);
  }

  if (block.type === "sql") {
    content.push(
      <Fragment key={block.id}>
        <span className="text-bold text-sm">SQL Query</span>
        <BlockMarkdown>{"```sql\n" + block.sql_query + "\n```"}</BlockMarkdown>
        <span className="text-bold text-sm">Results</span>
        <TableVirtualized key={block.id} table_id="0" tables={[block.result]} />
      </Fragment>,
    );
  }

  if (block.type === "viz") {
    content.push(
      <Fragment key={block.id}>
        <BlockMarkdown>
          {"```json\n" + JSON.stringify(block.config, null, 2) + "\n```"}
        </BlockMarkdown>
      </Fragment>,
    );
  }

  if (block.children && block.children.length > 0) {
    for (const childId of block.children) {
      const childBlock = blocks[childId];
      if (childBlock) {
        content = [...content, ...blockContentTraverse(childBlock, blocks)];
      }
    }
  }
  return content;
};

export const BlockMarkdown = ({ children }: { children: string }) => (
  <Markdown>{children}</Markdown>
);
