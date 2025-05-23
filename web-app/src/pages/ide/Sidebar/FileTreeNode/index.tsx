import {
  SidebarMenuButton,
  SidebarMenuItem,
  SidebarMenuSub,
  SidebarMenuSubButton,
  SidebarMenuSubItem,
} from "@/components/ui/shadcn/sidebar";
import { FileTreeModel } from "@/types/file";
import { ChevronRight, ChevronDown, FilePlus, FolderPlus } from "lucide-react";
import FileNode from "./FileNode";
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuTrigger,
} from "@/components/ui/shadcn/context-menu";
import { Pencil, Trash2 } from "lucide-react";
import React from "react";
import NewNode, { CreationType } from "../NewNode";
import AlertDeleteDialog from "../AlertDeleteDialog";
import RenameNode from "../RenameNode";
import { cn } from "@/libs/shadcn/utils";

const FileTreeNode = ({ fileTree }: { fileTree: FileTreeModel }) => {
  if (fileTree.is_dir) {
    return <DirNode fileTree={fileTree} />;
  }

  return <FileNode fileTree={fileTree} />;
};

export default FileTreeNode;

const DirNode = ({ fileTree }: { fileTree: FileTreeModel }) => {
  const [isContextMenuOpen, setIsContextMenuOpen] = React.useState(false);
  const [isEditing, setIsEditing] = React.useState(false);
  const [showDeleteDialog, setShowDeleteDialog] = React.useState(false);
  const [isCreating, setIsCreating] = React.useState(false);
  const [creationType, setCreationType] = React.useState<CreationType>("file");
  const [pendingDelete, setPendingDelete] = React.useState(false);
  const renameInputRef = React.useRef<HTMLInputElement>(null);
  const newItemInputRef = React.useRef<HTMLInputElement>(null);
  const [isOpen, setIsOpen] = React.useState(false);

  const handleRename = () => {
    setIsEditing(true);
  };

  const handleDelete = () => {
    setPendingDelete(true);
    setIsContextMenuOpen(false);
  };

  const handleCreateFile = () => {
    setCreationType("file");
    setIsCreating(true);
    setIsOpen(true);
  };

  const handleCreateFolder = () => {
    setCreationType("folder");
    setIsCreating(true);
    setIsOpen(true);
  };

  return (
    <div>
      <AlertDeleteDialog
        fileTree={fileTree}
        visible={showDeleteDialog}
        setVisible={setShowDeleteDialog}
      />

      <ContextMenu
        onOpenChange={(open) => {
          setIsContextMenuOpen(open);
          if (!open && pendingDelete) {
            setShowDeleteDialog(true);
            setPendingDelete(false);
          }
        }}
      >
        <ContextMenuTrigger asChild>
          <SidebarMenuItem key={fileTree.name}>
            <SidebarMenuButton
              onClick={() => {
                if (isCreating) return;
                if (isEditing) return;
                setIsOpen(!isOpen);
              }}
              className={cn(
                "overflow-visible",
                isContextMenuOpen ? "border border-border" : "",
              )}
            >
              {isEditing ? (
                <RenameNode
                  ref={renameInputRef}
                  fileTree={fileTree}
                  onRenamed={() => setIsEditing(false)}
                  onCancel={() => setIsEditing(false)}
                />
              ) : (
                <>
                  {isOpen ? (
                    <ChevronDown className="h-4 w-4" />
                  ) : (
                    <ChevronRight className="h-4 w-4" />
                  )}
                  <span>{fileTree.name}</span>
                </>
              )}
            </SidebarMenuButton>

            {isOpen && (
              <SidebarMenuSub className="translate-none">
                {isCreating && (
                  <NewNode
                    ref={newItemInputRef}
                    creationType={creationType}
                    currentPath={fileTree.path}
                    onCreated={() => setIsCreating(false)}
                    onCancel={() => setIsCreating(false)}
                  />
                )}
                {fileTree.children?.map((f) => (
                  <SidebarMenuSubItem key={f.path}>
                    <SidebarMenuSubButton asChild className="translate-none">
                      <FileTreeNode fileTree={f} />
                    </SidebarMenuSubButton>
                  </SidebarMenuSubItem>
                ))}
              </SidebarMenuSub>
            )}
          </SidebarMenuItem>
        </ContextMenuTrigger>
        <ContextMenuContent
          onCloseAutoFocus={(event: Event) => {
            if (renameInputRef.current) {
              renameInputRef.current.focus();
              renameInputRef.current.select();
              event.preventDefault();
            }
            if (newItemInputRef.current) {
              newItemInputRef.current.focus();
              newItemInputRef.current.select();
              event.preventDefault();
            }
          }}
        >
          <ContextMenuItem
            className="cursor-pointer"
            onClick={handleCreateFile}
          >
            <FilePlus className="mr-2 h-4 w-4" />
            <span>New File</span>
          </ContextMenuItem>
          <ContextMenuItem
            className="cursor-pointer"
            onClick={handleCreateFolder}
          >
            <FolderPlus className="mr-2 h-4 w-4" />
            <span>New Folder</span>
          </ContextMenuItem>
          <ContextMenuItem className="cursor-pointer" onClick={handleRename}>
            <Pencil className="mr-2 h-4 w-4" />
            <span>Rename</span>
          </ContextMenuItem>
          <ContextMenuItem
            className="text-red-600 cursor-pointer"
            onClick={handleDelete}
          >
            <Trash2 className="mr-2 h-4 w-4" />
            <span>Delete</span>
          </ContextMenuItem>
        </ContextMenuContent>
      </ContextMenu>
    </div>
  );
};
