import { create } from "zustand";
import type { Project } from "@/types/project";

interface CurrentProjectState {
  project: Project | null;
  setProject: (project: Project | null) => void;
}

const useCurrentProject = create<CurrentProjectState>()((set) => ({
  project: null,
  setProject: (project: Project | null) => set({ project })
}));

export default useCurrentProject;
