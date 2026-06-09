import { useMemo } from "react";
import type { FileTreeModel } from "@/types/file";
import useFileTree from "./files/useFileTree";

export interface ViewFileOption {
  value: string;
  label: string;
  path: string;
  searchText: string;
}

function flattenViewFiles(nodes: FileTreeModel[], result: ViewFileOption[] = []): ViewFileOption[] {
  for (const node of nodes) {
    if (node.is_dir) {
      flattenViewFiles(node.children, result);
    } else if (node.name.endsWith(".view.yml") || node.name.endsWith(".view.yaml")) {
      const viewName = node.name.replace(/\.view\.ya?ml$/, "");
      result.push({
        value: viewName,
        label: viewName,
        path: node.path,
        searchText: `${viewName} ${node.path}`.toLowerCase()
      });
    }
  }
  return result;
}

export default function useViewFiles() {
  const { data: fileTree, isLoading, error } = useFileTree();

  const viewFiles = useMemo(() => {
    if (!fileTree?.primary) return [];
    return flattenViewFiles(fileTree.primary);
  }, [fileTree]);

  return { viewFiles, isLoading, error };
}
