import { useMemo } from "react";
import type { FileTreeModel } from "@/types/file";
import useFileTree from "./files/useFileTree";

export interface TopicFileOption {
  value: string; // Topic name: "foo" (what gets stored in the form)
  label: string; // Display name: "foo"
  path: string; // Full path: "semantics/topics/foo.topic.yml" (for API calls)
  searchText: string; // For filtering: "foo semantics/topics/foo.topic.yml"
}

function flattenTopicFiles(
  nodes: FileTreeModel[],
  result: TopicFileOption[] = []
): TopicFileOption[] {
  for (const node of nodes) {
    if (node.is_dir) {
      flattenTopicFiles(node.children, result);
    } else if (node.name.endsWith(".topic.yml") || node.name.endsWith(".topic.yaml")) {
      const topicName = node.name.replace(/\.topic\.ya?ml$/, "");
      result.push({
        value: topicName,
        label: topicName,
        path: node.path,
        searchText: `${topicName} ${node.path}`.toLowerCase()
      });
    }
  }
  return result;
}

export default function useTopicFiles() {
  const { data: fileTree, isLoading, error } = useFileTree();

  const topicFiles = useMemo(() => {
    if (!fileTree) return [];
    return flattenTopicFiles(fileTree);
  }, [fileTree]);

  // Helper to find path by topic name
  const getPathByTopicName = (topicName: string): string | undefined => {
    return topicFiles.find((t) => t.value === topicName)?.path;
  };

  return {
    topicFiles,
    getPathByTopicName,
    isLoading,
    error
  };
}
