import { keepPreviousData, useMutation, useQuery } from "@tanstack/react-query";
import type { PaginationState } from "@tanstack/react-table";
import { useMemo } from "react";
import { createSearchParams, useLocation, useNavigate } from "react-router-dom";
import queryKeys from "@/hooks/api/queryKey";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { RunService } from "@/services/api";
import type { Block, LogItem, RetryType, TaskRun } from "@/services/types";
import { useBlockStore } from "@/stores/block";
import type { GroupSlice } from "@/stores/slices/group";
import useWorkflow, { type TaskConfigWithId } from "@/stores/useWorkflow";

const taskRunSelector = (
  blockId: string,
  blocks: Record<string, Block>,
  groupSlice: {
    groups: Record<string, GroupSlice["groups"][string]>;
    groupBlocks: Record<string, GroupSlice["groupBlocks"][string]>;
  }
): TaskRun[] => {
  const block = blocks[blockId];
  if (block?.type !== "task") {
    return [];
  }
  return [
    {
      id: block.id,
      name: block.task_name,
      isStreaming: block.is_streaming,
      error: block.error,
      loopIndex: block.task_metadata?.type === "loop_item" ? block.task_metadata.index : undefined,
      loopValue: block.task_metadata?.type === "loop" ? block.task_metadata.values : undefined,
      subWorkflowRunId:
        block.task_metadata?.type === "sub_workflow" ? block.task_metadata.run_id : undefined,
      children: block.children
    },
    ...block.children.flatMap((childId) => {
      return taskRunSelector(childId, blocks, groupSlice);
    })
  ];
};

const tasksSelector =
  (groupSlice: {
    groups: Record<string, GroupSlice["groups"][string]>;
    groupBlocks: Record<string, GroupSlice["groupBlocks"][string]>;
  }) =>
  (groupId: string): TaskRun[] => {
    const group = groupSlice.groups[groupId];
    const blocks = groupSlice.groupBlocks[groupId]?.blocks ?? {};
    const root = groupSlice.groupBlocks[groupId]?.root ?? [];
    if (!group) {
      return [];
    }
    return root.flatMap((blockId) => {
      return taskRunSelector(blockId, blocks, groupSlice);
    });
  };

const logSelector = (
  blockId: string,
  blocks: Record<string, Block>,
  groupSlice: {
    groups: Record<string, GroupSlice["groups"][string]>;
    groupBlocks: Record<string, GroupSlice["groupBlocks"][string]>;
  }
): LogItem[] => {
  const block = blocks[blockId];
  switch (block?.type) {
    case "group": {
      return [
        {
          content: `Group started: ${block.group_id}`,
          log_type: "info",
          timestamp: new Date().toISOString(),
          append: false,
          children: logsSelector(block.group_id)(groupSlice),
          is_streaming: block.is_streaming
        }
      ];
    }
    case "task": {
      const children = block.children.flatMap((childId) => {
        return logSelector(childId, blocks, groupSlice);
      });
      if (block.error) {
        children.push({
          content: `Error in ${block.type} run: ${block.error}`,
          log_type: "error",
          timestamp: new Date().toISOString(),
          append: false
        });
      }
      return [
        {
          content: `Task started: ${block.task_name}`,
          log_type: "info",
          timestamp: new Date().toISOString(),
          append: false,
          children: children,
          is_streaming: block.is_streaming
        }
      ];
    }
    case "text": {
      return [
        {
          content: block.content,
          log_type: "info",
          timestamp: new Date().toISOString(),
          append: false,
          is_streaming: block.is_streaming
        }
      ];
    }
    case "sql": {
      return [
        {
          content: `SQL Query\n${"```sql\n"}${block.sql_query}${"\n```"}`,
          log_type: "info",
          timestamp: new Date().toISOString(),
          append: false,
          is_streaming: block.is_streaming
        },
        {
          content: `Result\n\n${block.result
            .map((row, index) => {
              const separator = index === 0 ? `\n${"|---".repeat(row.length)}|` : "";

              return `|${row.join("|")}|${separator}`;
            })
            .join("\n")}\n`,
          log_type: "info",
          timestamp: new Date().toISOString(),
          append: false,
          is_streaming: block.is_streaming
        }
      ];
    }
    default: {
      return [];
    }
  }
};

const logsSelector =
  (groupKey: string) =>
  (groupSlice: {
    groups: Record<string, GroupSlice["groups"][string]>;
    groupBlocks: Record<string, GroupSlice["groupBlocks"][string]>;
  }): LogItem[] => {
    const group = groupSlice.groups[groupKey];
    const blocks = groupSlice.groupBlocks[groupKey]?.blocks ?? {};
    const root = groupSlice.groupBlocks[groupKey]?.root ?? [];

    if (!group) {
      return [];
    }
    let groupLogContent = `Group started: ${group.id}`;
    switch (group.type) {
      case "workflow":
        groupLogContent = `Workflow started: ${group.workflow_id}`;
        break;
      case "artifact":
        groupLogContent = `:::artifact{id=${group.artifact_id} title=${group.artifact_name} kind=${group.artifact_metadata.type} verified=${group.is_verified}}\n:::`;
        break;
      default:
        break;
    }

    return [
      {
        content: groupLogContent,
        log_type: "info",
        timestamp: new Date().toISOString(),
        append: false,
        is_streaming: group.is_streaming || false
      },

      ...root.flatMap((blockId) => {
        return logSelector(
          blockId,
          group.type === "artifact"
            ? Object.keys(blocks)
                .filter((id) => id === root[root.length - 1])
                .reduce(
                  (acc, id) => {
                    acc[id] = blocks[id];
                    return acc;
                  },
                  {} as Record<string, Block>
                )
            : blocks,
          groupSlice
        );
      }),
      ...(group.error
        ? [
            {
              content: `Error in ${group.type} run: ${group.error}`,
              log_type: "error",
              timestamp: new Date().toISOString(),
              append: false
            } as LogItem
          ]
        : [])
    ];
  };

export const useWorkflowLogs = (workflowId: string, runId: string) => {
  const groupId = getGroupId(workflowId, runId);
  const groupBlocks = useBlockStore((state) => state.groupBlocks);
  const groups = useBlockStore((state) => state.groups);
  return useMemo(() => {
    return logsSelector(groupId)({
      groupBlocks,
      groups
    });
  }, [groupId, groupBlocks, groups]);
};

export const useIsProcessing = (workflowId: string, runId: string) => {
  return useBlockStore((state) => {
    const groupId = getGroupId(workflowId, runId);
    const groupBlocks = state.groupBlocks[groupId];
    const isStreamProcessing = state.processingGroups[groupId] || false;
    const hasBlocksInStack = groupBlocks ? groupBlocks.blockStack.length > 0 : false;
    return isStreamProcessing || hasBlocksInStack;
  });
};

export const useSelectedLoopIndex = (task?: TaskConfigWithId) => {
  const selectedIndexes = useBlockStore((state) => state.selectedIndexes);
  if (!task) {
    return undefined;
  }
  const groupId = task.runId ? `${task.workflowId}::${task.runId}` : task.workflowId;
  const selectedId = `${groupId}.${task.id}`;
  return selectedIndexes[selectedId] || 0;
};

const taskRunIdSelector =
  (selectedIndexes: Record<string, number | undefined>) =>
  (task: TaskConfigWithId, groupId: string) => {
    let taskId = task.id || "";
    if (task.subWorkflowTaskId && taskId.startsWith(task.subWorkflowTaskId)) {
      // trim the subWorkflowTaskId from the taskId
      taskId = taskId.substring(task.subWorkflowTaskId.length + 1);
    }
    const result = taskId.split(".").reduce(
      (acc, part) => {
        const currentTaskId = acc.taskId ? `${acc.taskId}.${part}` : part;
        const currentRunId =
          acc.prevLoopIndex !== undefined ? `${part}-${acc.prevLoopIndex}` : part;
        const accTaskRunId = acc.taskRunId ? `${acc.taskRunId}.${currentRunId}` : currentRunId;
        return {
          prevLoopIndex: selectedIndexes[`${groupId}.${currentTaskId}`] || 0,
          taskId: currentTaskId,
          taskRunId: accTaskRunId
        };
      },
      {
        taskId: "",
        taskRunId: "",
        prevLoopIndex: undefined
      } as {
        taskId: string;
        taskRunId: string;
        prevLoopIndex?: number;
      }
    );
    return result.taskRunId;
  };

export const useTaskRun = (task: TaskConfigWithId) => {
  const selectedIndexes = useBlockStore((state) => state.selectedIndexes);
  const nodes = useWorkflow((state) => state.nodes);
  const groupBlocks = useBlockStore((state) => state.groupBlocks);
  const groups = useBlockStore((state) => state.groups);

  const taskRunsSelectorFn = tasksSelector({
    groups,
    groupBlocks
  });
  const taskRunIdSelectorFn = taskRunIdSelector(selectedIndexes);

  let currentLookup = task;

  // Find the correct runId for nested sub-workflows
  const subWorkflowNodes = [task];
  while (currentLookup.subWorkflowTaskId) {
    const subWorkflowNode = nodes.find((n) => n.id === task.subWorkflowTaskId);
    if (!subWorkflowNode) {
      break;
    }
    currentLookup = subWorkflowNode.data.task;
    subWorkflowNodes.push(currentLookup);
  }
  const lastNode = subWorkflowNodes.reverse().reduce(
    (acc, task) => {
      const groupId = getGroupId(task.workflowId, acc.scopeRunId || task.runId);
      const taskRunId = taskRunIdSelectorFn(task, groupId);
      const taskRuns = taskRunsSelectorFn(groupId);
      const taskRun = taskRuns.find((run) => run.id === taskRunId);
      let loopRuns: TaskRun[] = [];
      if (taskRun?.loopValue) {
        loopRuns = taskRun.children
          .map((childId) => {
            const childRun = taskRuns.find((run) => run.id === childId);
            return childRun;
          })
          .filter((run) => !!run);
      }

      return {
        taskRun,
        groupId,
        scopeRunId: taskRun?.subWorkflowRunId?.toString() || acc.scopeRunId,
        runId: acc.scopeRunId || task.runId,
        taskRunId,
        loopRuns
      };
    },
    {
      taskRun: undefined,
      groupId: "",
      runId: undefined,
      scopeRunId: undefined,
      taskRunId: "",
      loopRuns: []
    } as {
      taskRun?: TaskRun;
      groupId: string;
      runId?: string;
      scopeRunId?: string;
      taskRunId: string;
      loopRuns: TaskRun[];
    }
  );
  return {
    taskRun: lastNode.taskRun,
    taskRunId: lastNode.taskRunId,
    runId: lastNode.runId,
    loopRuns: lastNode.loopRuns
  };
};

export const useWorkflowRun = () => {
  const location = useLocation();
  const { project, branchName } = useCurrentProjectBranch();
  const navigate = useNavigate();
  const setGroupBlocks = useBlockStore((state) => state.setGroupBlocks);

  return useMutation({
    mutationFn: async ({ workflowId, retryType }: { workflowId: string; retryType: RetryType }) => {
      return await RunService.createRun(project.id, branchName, {
        type: "workflow",
        workflowId,
        retryType
      });
    },
    onSuccess(data) {
      const runIndex = data.run.run_index;
      setGroupBlocks(data.run, {}, []);
      navigate({
        pathname: location.pathname,
        search: createSearchParams({
          run: runIndex.toString()
        }).toString()
      });
    }
  });
};

export const useCancelWorkflowRun = () => {
  const { project, branchName } = useCurrentProjectBranch();
  return useMutation({
    mutationFn: async ({ sourceId, runIndex }: { sourceId: string; runIndex: number }) => {
      return await RunService.cancelRun(project.id, branchName, sourceId, runIndex);
    }
  });
};

export const useDeleteWorkflowRun = () => {
  const { project, branchName } = useCurrentProjectBranch();
  return useMutation({
    mutationFn: async ({ workflowId, runIndex }: { workflowId: string; runIndex: number }) => {
      return await RunService.deleteRun(project.id, branchName, workflowId, runIndex);
    }
  });
};

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
      return await RunService.streamEvents(
        project.id,
        branchName,
        {
          sourceId,
          runIndex
        },
        handleEvent,
        () => {
          cleanupGroupStacks("Cancelled");
          const groupId = getGroupId(sourceId, runIndex.toString());
          setGroupProcessing(groupId, false);
        },
        (error) => {
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

  return {
    stream: mutation
  };
};

export const useGetBlocks = (sourceId: string, runIndex?: number, enabled?: boolean) => {
  const { project, branchName } = useCurrentProjectBranch();

  return useQuery({
    queryKey: queryKeys.workflow.getBlocks(project.id, sourceId, branchName, runIndex),
    queryFn: async () => {
      return await RunService.getBlocks(project.id, branchName, {
        source_id: sourceId,
        run_index: runIndex
      });
    },
    enabled
  });
};

export const useListWorkflowRuns = (workflowId: string, pagination: PaginationState) => {
  const { project, branchName } = useCurrentProjectBranch();
  return useQuery({
    queryKey: queryKeys.workflow.getRuns(project.id, branchName, workflowId, pagination),
    queryFn: async () => {
      const response = await RunService.listRuns(project.id, branchName, workflowId, pagination);
      return response;
    },
    enabled: !!workflowId,
    placeholderData: keepPreviousData
  });
};

const getGroupId = (sourceId: string, runId?: string): string => {
  return runId ? `${sourceId}::${runId}` : sourceId;
};
