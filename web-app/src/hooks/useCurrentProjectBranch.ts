import useCurrentWorkspaceBranch from "./useCurrentWorkspaceBranch";

export default function useCurrentProjectBranch() {
  const { workspace, ...rest } = useCurrentWorkspaceBranch();
  return { project: workspace, workspace, ...rest };
}
