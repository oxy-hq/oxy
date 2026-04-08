import { create } from "zustand";
import type { Workspace } from "@/types/workspace";

interface CurrentWorkspaceState {
  workspace: Workspace | null;
  setWorkspace: (workspace: Workspace | null) => void;
}

const useCurrentWorkspace = create<CurrentWorkspaceState>()((set) => ({
  workspace: null,
  setWorkspace: (workspace: Workspace | null) => set({ workspace })
}));

export default useCurrentWorkspace;
