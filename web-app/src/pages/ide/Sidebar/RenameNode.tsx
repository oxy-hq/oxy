import { FileTreeModel } from "@/types/file";
import { ChevronRight } from "lucide-react";
import React from "react";
import useRenameFolder from "@/hooks/api/files/useRenameFolder";
import { useLocation, useNavigate } from "react-router-dom";
import useRenameFile from "@/hooks/api/files/useRenameFile";
import { File } from "lucide-react";
import useFileTree from "@/hooks/api/files/useFileTree";
import { toast } from "sonner";

interface RenameNodeProps {
  fileTree: FileTreeModel;
  onRenamed: () => void;
  onCancel: () => void;
}

const RenameNode = React.forwardRef<HTMLInputElement, RenameNodeProps>(
  ({ fileTree, onRenamed, onCancel }, ref) => {
    const { pathname } = useLocation();
    const { data } = useFileTree();
    const [editingName, setEditingName] = React.useState(fileTree.name);
    const renameFolder = useRenameFolder();
    const renameFile = useRenameFile();
    const navigate = useNavigate();
    const [error, setError] = React.useState(false);

    const onValidateName = (newPath: string) => {
      const pathExistsInTree = (items: FileTreeModel[]): boolean => {
        for (const item of items) {
          if (item.path === newPath) return true;
          if (item.children?.length) {
            if (pathExistsInTree(item.children)) return true;
          }
        }
        return false;
      };

      return !pathExistsInTree(data || []);
    };

    const isDir = fileTree.is_dir;

    const handleRename = async () => {
      if (editingName !== fileTree.name) {
        try {
          const dirPath = fileTree.path.substring(
            0,
            fileTree.path.lastIndexOf("/") + 1,
          );
          const newPath = dirPath + editingName;

          if (!onValidateName(newPath)) {
            setError(true);
            return;
          }

          if (isDir) {
            await renameFolder.mutateAsync({
              pathb64: btoa(fileTree.path),
              newName: newPath,
            });
          } else {
            await renameFile.mutateAsync({
              pathb64: btoa(fileTree.path),
              newName: newPath,
            });
          }
          const currentPath = atob(pathname.split("/").pop() ?? "");
          if (currentPath.startsWith(fileTree.path)) {
            const newUrl = currentPath.replace(fileTree.path, newPath);
            navigate(`/ide/${btoa(newUrl)}`);
          }
          setError(false);
          onRenamed();
        } catch (error) {
          toast.error("Failed to rename", {
            description: "There was a problem with your request.",
          });
          console.error("Failed to rename:", error);
          setEditingName(fileTree.name);
        }
      } else {
        onRenamed();
      }
    };

    const handleKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
      if (e.key === "Enter") {
        handleRename();
      }
      if (e.key === "Escape") {
        onCancel();
      }
    };

    return (
      <div className="flex flex-col w-full">
        <div className="flex items-center gap-2 w-full text-sm p-2">
          {isDir ? (
            <ChevronRight className="h-4 w-4" />
          ) : (
            <File className="h-4 w-4" />
          )}
          <div className="relative flex-1">
            <input
              ref={ref}
              onBlur={onCancel}
              className={`w-full bg-transparent border border-2 shadow-none outline-none ${
                error ? "border-red-500" : "border-gray-600"
              }`}
              value={editingName}
              onChange={(e) => {
                setEditingName(e.target.value);
                setError(false);
              }}
              onKeyDown={handleKeyDown}
            />
            {error && (
              <div className="text-xs text-white absolute left-0 top-full w-full z-10 bg-red-500 p-1">
                A {isDir ? "folder" : "file"} with this name already exists
              </div>
            )}
          </div>
        </div>
      </div>
    );
  },
);

export default RenameNode;
