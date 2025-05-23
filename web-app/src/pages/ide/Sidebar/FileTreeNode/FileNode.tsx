import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
} from "@/components/ui/shadcn/context-menu";
import { ContextMenuTrigger } from "@/components/ui/shadcn/context-menu";
import {
  SidebarMenuItem,
  SidebarMenuButton,
} from "@/components/ui/shadcn/sidebar";
import { FileTreeModel } from "@/types/file";
import { Pencil, Trash2, File } from "lucide-react";
import React from "react";
import { useLocation, Link } from "react-router-dom";
import AlertDeleteDialog from "../AlertDeleteDialog";
import RenameNode from "../RenameNode";
import { cn } from "@/libs/shadcn/utils";
const FileNode = ({ fileTree }: { fileTree: FileTreeModel }) => {
  const { pathname } = useLocation();
  const isActive = pathname === `/ide/${btoa(fileTree.path)}`;
  const [isContextMenuOpen, setIsContextMenuOpen] = React.useState(false);
  const [isEditing, setIsEditing] = React.useState(false);
  const inputRef = React.useRef<HTMLInputElement>(null);
  const [showDeleteDialog, setShowDeleteDialog] = React.useState(false);
  const handleRename = () => {
    setIsEditing(true);
  };

  const handleDelete = () => {
    setShowDeleteDialog(true);
  };

  return (
    <>
      <AlertDeleteDialog
        fileTree={fileTree}
        visible={showDeleteDialog}
        setVisible={setShowDeleteDialog}
      />

      <ContextMenu onOpenChange={setIsContextMenuOpen}>
        <ContextMenuTrigger asChild>
          <SidebarMenuItem key={fileTree.name}>
            <SidebarMenuButton
              asChild
              isActive={isActive}
              className={cn(
                "overflow-visible",
                isContextMenuOpen ? "border border-border" : "",
              )}
            >
              {isEditing ? (
                <RenameNode
                  ref={inputRef}
                  fileTree={fileTree}
                  onRenamed={() => setIsEditing(false)}
                  onCancel={() => setIsEditing(false)}
                />
              ) : (
                <Link to={`/ide/${btoa(fileTree.path)}`}>
                  <File />
                  <span>{fileTree.name}</span>
                </Link>
              )}
            </SidebarMenuButton>
          </SidebarMenuItem>
        </ContextMenuTrigger>
        <ContextMenuContent
          onCloseAutoFocus={(event) => {
            if (inputRef.current) {
              inputRef.current.focus();
              inputRef.current.select();
              event.preventDefault();
            }
          }}
        >
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
    </>
  );
};

export default FileNode;
