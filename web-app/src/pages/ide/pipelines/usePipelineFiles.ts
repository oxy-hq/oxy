import { useMemo } from "react";
import useFileTree from "@/hooks/api/files/useFileTree";
import type { FileTreeModel } from "@/types/file";

export interface PipelineFile {
  /** Display name without the `.airway.yml` suffix. */
  name: string;
  /** Repo-relative path, e.g. `pipelines/shopify_raw.airway.yml`. */
  path: string;
}

const PIPELINE_SUFFIX = /\.airway\.(yml|yaml)$/;

function collect(nodes: FileTreeModel[], acc: PipelineFile[]): void {
  for (const node of nodes) {
    if (node.is_dir) {
      collect(node.children ?? [], acc);
    } else if (PIPELINE_SUFFIX.test(node.name)) {
      acc.push({ name: node.name.replace(PIPELINE_SUFFIX, ""), path: node.path });
    }
  }
}

/** All `.airway.yml` / `.airway.yaml` files in the workspace, sorted by name. */
export default function usePipelineFiles() {
  const { data: fileTree, isLoading, refetch } = useFileTree();

  const pipelines = useMemo(() => {
    const acc: PipelineFile[] = [];
    collect(fileTree?.primary ?? [], acc);
    acc.sort((a, b) => a.name.localeCompare(b.name));
    return acc;
  }, [fileTree]);

  return { pipelines, isLoading, refetch };
}
