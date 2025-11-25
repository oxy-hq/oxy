import React from "react";
import { Input } from "@/components/ui/shadcn/input";
import useFileTree from "@/hooks/api/files/useFileTree";
import { FileTreeModel } from "@/types/file";

interface FilePathAutocompleteInputProps
  extends Omit<React.ComponentProps<typeof Input>, "list"> {
  fileExtension?: string | string[];
  datalistId: string;
}

const flattenFileTree = (
  tree: FileTreeModel[],
  extensions?: string | string[],
): string[] => {
  const paths: string[] = [];

  let extensionList: string[] | null = null;
  if (extensions) {
    extensionList = Array.isArray(extensions) ? extensions : [extensions];
  }

  const traverse = (node: FileTreeModel) => {
    if (!node.is_dir) {
      if (
        !extensionList ||
        extensionList.some((ext) => node.path.endsWith(ext))
      ) {
        paths.push(node.path);
      }
    }
    if (node.children && node.children.length > 0) {
      node.children.forEach(traverse);
    }
  };

  tree.forEach(traverse);
  return paths;
};

export const FilePathAutocompleteInput = React.forwardRef<
  HTMLInputElement,
  FilePathAutocompleteInputProps
>(({ fileExtension, datalistId, ...props }, ref) => {
  const { data: fileTree } = useFileTree();

  const filePaths = React.useMemo(() => {
    if (!fileTree) return [];
    return flattenFileTree(fileTree, fileExtension);
  }, [fileTree, fileExtension]);

  return (
    <>
      <Input ref={ref} list={datalistId} {...props} />
      <datalist id={datalistId}>
        {filePaths.map((path) => (
          <option key={path} value={path} />
        ))}
      </datalist>
    </>
  );
});

FilePathAutocompleteInput.displayName = "FilePathAutocompleteInput";
