import { create } from "zustand";
import { persistNSync } from "persist-and-sync";

interface IdeBranchState {
  // Map of project ID to selected branch name
  projectBranches: Record<string, string>;

  // Get the current branch for a project (defaults to active branch if not set)
  getCurrentBranch: (
    projectId: string,
    activeBranchName?: string,
  ) => string | undefined;

  // Set the current branch for a project
  setCurrentBranch: (projectId: string, branchName: string) => void;

  // Clear branch data for a project
  clearProjectBranch: (projectId: string) => void;
}

const useIdeBranch = create<IdeBranchState>()(
  persistNSync(
    (set, get) => ({
      projectBranches: {},

      getCurrentBranch: (projectId: string, activeBranchName?: string) => {
        const { projectBranches } = get();
        return projectBranches[projectId] || activeBranchName;
      },

      setCurrentBranch: (projectId: string, branchName: string) => {
        set((state) => ({
          projectBranches: {
            ...state.projectBranches,
            [projectId]: branchName,
          },
        }));
      },

      clearProjectBranch: (projectId: string) => {
        set((state) => {
          const newProjectBranches = { ...state.projectBranches };
          delete newProjectBranches[projectId];
          return { projectBranches: newProjectBranches };
        });
      },
    }),
    {
      name: "ide-branch-storage",
    },
  ),
);

export default useIdeBranch;
