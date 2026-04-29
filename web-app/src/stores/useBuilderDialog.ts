import { create } from "zustand";
import type { NodeSummary } from "@/types/modeling";

interface ModelingSelection {
  projectName: string;
  node: NodeSummary | null;
}

interface BuilderDialogState {
  isOpen: boolean;
  setIsOpen: (isOpen: boolean) => void;
  modelingSelection: ModelingSelection | null;
  setModelingSelection: (selection: ModelingSelection | null) => void;
}

const useBuilderDialog = create<BuilderDialogState>()((set) => ({
  isOpen: false,
  setIsOpen: (isOpen: boolean) => set({ isOpen }),
  modelingSelection: null,
  setModelingSelection: (selection) => set({ modelingSelection: selection })
}));

export default useBuilderDialog;
