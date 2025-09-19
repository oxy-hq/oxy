import {
  SidebarMenuButton,
  SidebarMenuItem,
} from "@/components/ui/shadcn/sidebar";
import { FilePlus, FolderPlus } from "lucide-react";
import React from "react";
import useCreateFile from "@/hooks/api/files/useCreateFile";
import useCreateFolder from "@/hooks/api/files/useCreateFolder";
import { useNavigate } from "react-router-dom";
import useFileTree from "@/hooks/api/files/useFileTree";
import { FileTreeModel } from "@/types/file";
import { toast } from "sonner";
import ROUTES from "@/libs/utils/routes";
import useCurrentProject from "@/stores/useCurrentProject";

export type CreationType = "file" | "folder";

interface NewNodeProps {
  currentPath?: string;
  creationType: CreationType;
  onCreated: () => void;
  onCancel: () => void;
}

const NewNode = React.forwardRef<HTMLInputElement, NewNodeProps>(
  ({ creationType, onCreated, onCancel, currentPath }, ref) => {
    const { data } = useFileTree();
    const { project } = useCurrentProject();
    const [newItemName, setNewItemName] = React.useState("");
    const createFile = useCreateFile();
    const createFolder = useCreateFolder();
    const navigate = useNavigate();
    const [error, setError] = React.useState(false);
    if (!project) {
      throw new Error("Project ID is required");
    }

    const projectId = project.id;

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

    const handleCreate = async () => {
      if (!newItemName) return;

      try {
        const newPath = currentPath
          ? `${currentPath}/${newItemName}`
          : newItemName;

        if (!onValidateName(newPath)) {
          setError(true);
          return;
        }

        if (creationType === "file") {
          await createFile.mutateAsync(btoa(newPath));
          const ideUri = ROUTES.PROJECT(projectId).IDE.FILE(btoa(newPath));
          navigate(ideUri);
        } else {
          await createFolder.mutateAsync(btoa(newPath));
        }

        onCreated();
      } catch (error) {
        toast.error("Failed to create", {
          description: "There was a problem with your request.",
        });
        console.error("Failed to create", error);
      }
    };

    const handleKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
      if (e.key === "Enter") {
        handleCreate();
      } else if (e.key === "Escape") {
        onCancel();
      }
    };

    return (
      <SidebarMenuItem>
        <SidebarMenuButton className="overflow-visible">
          {creationType === "file" ? <FilePlus /> : <FolderPlus />}
          <div className="relative flex-1">
            <input
              autoFocus
              ref={ref}
              onBlur={onCancel}
              className={`w-full bg-transparent border border-2 shadow-none outline-none ${
                error ? "border-red-500" : "border-gray-600"
              }`}
              value={newItemName}
              onChange={(e) => {
                setNewItemName(e.target.value);
                setError(false);
              }}
              onKeyDown={handleKeyDown}
            />
            {error && (
              <div className="text-xs text-white absolute left-0 top-full w-full z-10 bg-red-500 p-1">
                A {creationType === "file" ? "file" : "folder"} with this name
                already exists
              </div>
            )}
          </div>
        </SidebarMenuButton>
      </SidebarMenuItem>
    );
  },
);

NewNode.displayName = "NewNode";

export default NewNode;
