import { StateCreator } from "zustand";
import { GroupSlice } from "./group";
import { TaskConfigWithId } from "../useWorkflow";

export interface SelectSlice {
  selectedIndexes: Record<string, number | undefined>;
  setSelectedLoopIndex: (task: TaskConfigWithId, index: number) => void;
  resetSelectedIndexes: () => void;
}

export const createSelectSlice: StateCreator<
  GroupSlice & SelectSlice,
  [["zustand/devtools", never]],
  [],
  SelectSlice
> = (set) => ({
  selectedIndexes: {},
  setSelectedLoopIndex: (task: TaskConfigWithId, index: number) =>
    set((state) => {
      const groupId = task.runId
        ? `${task.workflowId}::${task.runId}`
        : task.workflowId;
      const selectedId = `${groupId}.${task.id}`;
      const isSelected = state.selectedIndexes[selectedId] === index;
      return {
        selectedIndexes: {
          ...state.selectedIndexes,
          [selectedId]: isSelected ? undefined : index,
        },
      };
    }),
  resetSelectedIndexes: () =>
    set(() => ({
      selectedIndexes: {},
    })),
});
