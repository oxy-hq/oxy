import { Block, BlockContent, TextContent } from "@/services/types";

export interface BlockSlice {
  error?: string;
  blocks: Record<string, Block>;
  blockStack: string[];
  root: string[];
  removeBlockStack: (blockId: string, error?: string) => void;
  upsertBlockToStack: (blockId: string, blockData: BlockContent) => void;
  addGroupBlock: (groupId: string) => void;
  handleError: (error: string) => void;
  cleanupBlockStacks: (error: string) => void;
}

export type BlockSliceSetter = (
  partial: (state: BlockSlice) => BlockSlice | Partial<BlockSlice>,
) => void;

export const createBlockSlice = (
  set: BlockSliceSetter,
  init?: Partial<BlockSlice>,
): BlockSlice => ({
  error: undefined,
  blocks: {},
  blockStack: [],
  root: [],
  ...init,
  removeBlockStack: (blockId, error) =>
    set((state) => {
      const block = state.blocks[blockId];
      return {
        ...state,
        blockStack: state.blockStack.filter((id) => id !== blockId),
        blocks: {
          ...state.blocks,
          [blockId]: {
            ...block,
            error,
            is_streaming: false,
          },
        },
      };
    }),
  upsertBlockToStack: (blockId: string, blockData: BlockContent) =>
    set((state) => {
      const parentBlockId = state.blockStack[state.blockStack.length - 1];
      const parentBlock = parentBlockId
        ? state.blocks[parentBlockId]
        : undefined;
      const blockStack = state.blockStack.includes(blockId)
        ? state.blockStack
        : [...state.blockStack, blockId];
      const existingBlock = state.blocks[blockId];
      let payload = blockData;
      if (existingBlock && existingBlock.type == "text") {
        payload = {
          ...existingBlock,
          ...payload,
          content: `${existingBlock.content}${(blockData as TextContent).content}`,
        } as TextContent;
      }
      const isNewRootChild = !parentBlockId && !existingBlock;

      return {
        ...state,
        root: isNewRootChild ? [...state.root, blockId] : state.root,
        blockStack,
        blocks: {
          ...state.blocks,
          ...(parentBlockId && parentBlock
            ? {
                [parentBlockId]: {
                  ...parentBlock,
                  children: parentBlock.children.includes(blockId)
                    ? parentBlock.children
                    : [...parentBlock.children, blockId],
                },
              }
            : {}),
          [blockId]: {
            ...existingBlock,
            ...payload,
            id: blockId,
            children: existingBlock ? existingBlock.children : [],
            is_streaming: true,
          },
        },
      };
    }),
  handleError: (error: string) =>
    set((state) => {
      return {
        ...state,
        error,
      };
    }),
  cleanupBlockStacks: (error: string) =>
    set((state) => {
      const blockStack = state.blockStack;
      const blocks = { ...state.blocks };
      for (const blockId of blockStack) {
        if (blocks[blockId]) {
          blocks[blockId] = {
            ...blocks[blockId],
            is_streaming: false,
            error,
          };
        }
      }
      return {
        ...state,
        blockStack: [],
        blocks,
      };
    }),
  addGroupBlock: (groupId: string) =>
    set((state) => {
      const currentBlockId = state.blockStack[state.blockStack.length - 1];
      const currentBlock = currentBlockId
        ? state.blocks[currentBlockId]
        : undefined;

      if (!currentBlock) {
        return state;
      }

      return {
        ...state,
        blocks: {
          ...state.blocks,
          [currentBlockId]: {
            ...currentBlock,
            children: currentBlock.children.includes(groupId)
              ? currentBlock.children
              : [...currentBlock.children, groupId],
          },
          [groupId]: {
            id: groupId,
            type: "group",
            group_id: groupId,
            children: [],
          },
        },
      };
    }),
});
