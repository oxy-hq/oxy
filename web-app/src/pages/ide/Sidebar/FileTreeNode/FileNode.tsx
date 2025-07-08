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
import { useReadonly } from "@/hooks/useReadonly";
const FileNode = ({ fileTree }: { fileTree: FileTreeModel }) => {
  const { pathname } = useLocation();
  const { isReadonly } = useReadonly();
  const isActive = pathname === `/ide/${btoa(fileTree.path)}`;
  const [isContextMenuOpen, setIsContextMenuOpen] = React.useState(false);
  const [isEditing, setIsEditing] = React.useState(false);
  const inputRef = React.useRef<HTMLInputElement>(null);
  const [showDeleteDialog, setShowDeleteDialog] = React.useState(false);
  const [pendingDelete, setPendingDelete] = React.useState(false);
  const handleRename = () => {
    if (isReadonly) return;
    setIsEditing(true);
  };

  const handleDelete = () => {
    if (isReadonly) return;
    setPendingDelete(true);
    setIsContextMenuOpen(false);
  };

  return (
    <>
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
          {!isReadonly && (
            <>
              <ContextMenuItem
                className="cursor-pointer"
                onClick={handleRename}
              >
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
            </>
          )}
          {isReadonly && (
            <ContextMenuItem disabled>
              <span className="text-muted-foreground">Read-only mode</span>
            </ContextMenuItem>
          )}
        </ContextMenuContent>
      </ContextMenu>
    </>
  );
};

export default FileNode;
