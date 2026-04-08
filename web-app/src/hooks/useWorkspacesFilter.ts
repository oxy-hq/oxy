import { useMemo } from "react";
import type { Workspace } from "@/types/workspace";

export const useWorkspacesFilter = (workspaces: Workspace[] | undefined, searchQuery: string) => {
  const filteredWorkspaces = useMemo(() => {
    if (!workspaces) return [];

    if (!searchQuery.trim()) return workspaces;

    return workspaces.filter((workspace) =>
      workspace.name.toLowerCase().includes(searchQuery.toLowerCase())
    );
  }, [workspaces, searchQuery]);

  return filteredWorkspaces;
};
