import { create } from "zustand";

interface SelectedRepoState {
  /** "primary" or the linked repo name */
  selectedRepo: string;
  setSelectedRepo: (name: string) => void;
}

const useSelectedRepo = create<SelectedRepoState>()((set) => ({
  selectedRepo: "primary",
  setSelectedRepo: (name) => set({ selectedRepo: name })
}));

export default useSelectedRepo;
