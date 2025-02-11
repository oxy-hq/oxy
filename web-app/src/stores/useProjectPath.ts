import { create } from "zustand";

interface ProjectPath {
  projectPath: string;
  setProjectPath: (projectPath: string) => void;
}

const useProjectPath = create<ProjectPath>((set) => ({
  projectPath: "",
  setProjectPath: (projectPath: string) =>
    set(() => ({
      projectPath: projectPath,
    })),
}));

export default useProjectPath;
