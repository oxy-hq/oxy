import { Project } from "@/types/project";
import { create } from "zustand";

interface CurrentProjectState {
  project: Project | null;
  setProject: (project: Project | null) => void;
}

const useCurrentProject = create<CurrentProjectState>()((set) => ({
  project: null,
  setProject: (project: Project | null) => set({ project }),
}));

export default useCurrentProject;
