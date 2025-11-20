import {
  Block,
  BlockContent,
  Group,
  GroupKind,
  RunInfo,
} from "@/services/types";
import { BlockSlice, BlockSliceSetter, createBlockSlice } from "./block";
import { StateCreator } from "zustand";

export interface GroupSlice {
  groupBlocks: Record<string, BlockSlice>;
  groupStack: string[];
  groups: Record<string, Group>;
  groupAliases: Record<string, string>;
  processingGroups: Record<string, boolean>;
  setGroupBlocks: (
    runInfo: RunInfo,
    blocks?: Record<string, Block>,
    children?: string[],
    error?: string,
    metadata?: GroupKind,
  ) => void;
  addGroup: (groupId: string, groupData: Group) => void;
  removeGroup: (groupId: string, error?: string) => void;
  upsertBlockToStack: (blockId: string, blockData: BlockContent) => void;
  removeBlockStack: (blockId: string, error?: string) => void;
  cleanupGroupStacks: (error: string) => void;
  setGroupProcessing: (groupId: string, isProcessing: boolean) => void;
}

const wrapChildrenSet =
  (
    set: {
      (
        partial:
          | GroupSlice
          | Partial<GroupSlice>
          | ((state: GroupSlice) => GroupSlice | Partial<GroupSlice>),
        replace?: false,
      ): void;
      (
        state: GroupSlice | ((state: GroupSlice) => GroupSlice),
        replace: true,
      ): void;
    },
    groupId: string,
  ): BlockSliceSetter =>
  (partial) =>
    set((state) => {
      const groupBlock = state.groupBlocks[groupId];
      const value = partial(groupBlock);
      return {
        groupBlocks: {
          ...state.groupBlocks,
          [groupId]: {
            ...groupBlock,
            ...value,
          },
        },
      };
    });

export const createGroupSlice: StateCreator<
  GroupSlice,
  [["zustand/devtools", never]],
  [],
  GroupSlice
> = (set, get) => ({
  groupBlocks: {},
  groupStack: [],
  groups: {},
  processingGroups: {},
  groupAliases: {},
  setGroupBlocks: (
    runInfo: RunInfo,
    blocks?: Record<string, Block>,
    children?: string[],
    error?: string,
    metadata?: GroupKind,
  ) =>
    set((state) => {
      const groupId = `${runInfo.source_id}::${runInfo.run_index}`;
      const groupData: Group = metadata
        ? {
            id: groupId,
            error,
            ...metadata,
          }
        : {
            id: groupId,
            type: "workflow",
            workflow_id: runInfo.source_id,
            run_id: runInfo.run_index.toString(),
            error,
          };

      if (state.groups[groupId]) {
        return {
          groups: {
            ...state.groups,
            [groupId]: groupData,
          },
          groupAliases: {
            ...state.groupAliases,
            ...(runInfo.lookup_id ? { [runInfo.lookup_id]: groupId } : {}),
          },
        };
      }

      return {
        groupBlocks: {
          ...state.groupBlocks,
          [groupId]: createBlockSlice(wrapChildrenSet(set, groupId), {
            blocks: blocks || {},
            root: children || [],
          }),
        },
        groupStack: [],
        groups: {
          ...state.groups,
          [groupId]: groupData,
        },
        groupAliases: {
          ...state.groupAliases,
          ...(runInfo.lookup_id ? { [runInfo.lookup_id]: groupId } : {}),
        },
      };
    }),
  addGroup: (groupId: string, groupData: Group) => {
    const groupStack = get().groupStack;
    const groupBlocks = get().groupBlocks;
    const currentGroup = groupStack[groupStack.length - 1];
    const currentGroupBlocks = currentGroup
      ? groupBlocks[currentGroup]
      : undefined;
    if (currentGroupBlocks) {
      currentGroupBlocks.addGroupBlock(groupId);
    }

    set((state) => {
      return {
        groupStack: state.groupStack.includes(groupId)
          ? state.groupStack
          : [...state.groupStack, groupId],
        groupBlocks: {
          ...state.groupBlocks,
          [groupId]: createBlockSlice(wrapChildrenSet(set, groupId)),
        },
        groups: {
          ...state.groups,
          [groupId]: groupData,
        },
      };
    });
  },
  removeGroup: (groupId: string, error?: string) =>
    set((state) => {
      return {
        groupStack: [...state.groupStack.filter((id) => id !== groupId)],
        groups: {
          ...state.groups,
          [groupId]: {
            ...state.groups[groupId],
            error,
          },
        },
      };
    }),
  upsertBlockToStack: (blockId: string, blockData: BlockContent) => {
    const groupStack = get().groupStack;
    const groupBlocks = get().groupBlocks;
    const currentGroup = groupStack[groupStack.length - 1];
    const currentGroupBlocks = currentGroup
      ? groupBlocks[currentGroup]
      : undefined;
    if (currentGroupBlocks) {
      currentGroupBlocks.upsertBlockToStack(blockId, blockData);
    }
  },
  removeBlockStack: (blockId: string, error?: string) => {
    const groupStack = get().groupStack;
    const groupBlocks = get().groupBlocks;
    const currentGroup = groupStack[groupStack.length - 1];
    const currentGroupBlocks = currentGroup
      ? groupBlocks[currentGroup]
      : undefined;
    if (currentGroupBlocks) {
      currentGroupBlocks.removeBlockStack(blockId, error);
    }
  },
  cleanupGroupStacks: (error: string) => {
    const groupStack = get().groupStack;
    const groupBlocks = get().groupBlocks;
    for (const groupId of groupStack) {
      const groupBlock = groupBlocks[groupId];
      if (groupBlock) {
        groupBlock.cleanupBlockStacks(error);
        get().removeGroup(groupId, error);
      }
    }
  },
  setGroupProcessing: (groupId: string, isProcessing: boolean) =>
    set((state) => ({
      processingGroups: {
        ...state.processingGroups,
        [groupId]: isProcessing,
      },
    })),
});
