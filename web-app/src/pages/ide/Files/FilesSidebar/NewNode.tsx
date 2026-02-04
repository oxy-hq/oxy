import { FilePlus, FolderPlus } from "lucide-react";
import React from "react";
import { useNavigate } from "react-router-dom";
import { toast } from "sonner";
import { SidebarMenuButton, SidebarMenuItem } from "@/components/ui/shadcn/sidebar";
import useCreateFile from "@/hooks/api/files/useCreateFile";
import useCreateFolder from "@/hooks/api/files/useCreateFolder";
import useFileTree from "@/hooks/api/files/useFileTree";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import ROUTES from "@/libs/utils/routes";
import type { FileTreeModel } from "@/types/file";

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
    const { project } = useCurrentProjectBranch();

    const [newItemName, setNewItemName] = React.useState("");
    const createFile = useCreateFile();
    const createFolder = useCreateFolder();
    const navigate = useNavigate();
    const [error, setError] = React.useState(false);

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
        const newPath = currentPath ? `${currentPath}/${newItemName}` : newItemName;

        if (!onValidateName(newPath)) {
          setError(true);
          return;
        }

        if (creationType === "file") {
          await createFile.mutateAsync(btoa(newPath));
          const ideUri = ROUTES.PROJECT(projectId).IDE.FILES.FILE(btoa(newPath));
          navigate(ideUri);
        } else {
          await createFolder.mutateAsync(btoa(newPath));
        }

        onCreated();
      } catch (error) {
        toast.error("Failed to create", {
          description: "There was a problem with your request."
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
        <SidebarMenuButton className='overflow-visible'>
          {creationType === "file" ? <FilePlus /> : <FolderPlus />}
          <div className='relative flex-1'>
            <input
              ref={ref}
              onBlur={onCancel}
              className={`w-full border border-2 bg-transparent shadow-none outline-none ${
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
              <div className='absolute top-full left-0 z-10 w-full bg-red-500 p-1 text-white text-xs'>
                A {creationType === "file" ? "file" : "folder"} with this name already exists
              </div>
            )}
          </div>
        </SidebarMenuButton>
      </SidebarMenuItem>
    );
  }
);

NewNode.displayName = "NewNode";

export default NewNode;
