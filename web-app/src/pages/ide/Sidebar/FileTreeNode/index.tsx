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
import { SIDEBAR_REVEAL_FILE } from "../events";
import NewNode, { CreationType } from "../NewNode";
import AlertDeleteDialog from "../AlertDeleteDialog";
import RenameNode from "../RenameNode";
import { cn } from "@/libs/shadcn/utils";
import { useReadonly } from "@/hooks/useReadonly";

const isDescendant = (p: string, base: string) =>
  p === base || p.startsWith(base + "/");

const FileTreeNode = ({
  fileTree,
  activePath,
}: {
  fileTree: FileTreeModel;
  activePath?: string;
}) => {
  if (fileTree.is_dir) {
    return <DirNode fileTree={fileTree} activePath={activePath} />;
  }

  return <FileNode fileTree={fileTree} activePath={activePath} />;
};

export default FileTreeNode;

const DirNode = ({
  fileTree,
  activePath,
}: {
  fileTree: FileTreeModel;
  activePath?: string;
}) => {
  const { isReadonly } = useReadonly();
  const [isContextMenuOpen, setIsContextMenuOpen] = React.useState(false);
  const [isEditing, setIsEditing] = React.useState(false);
  const [showDeleteDialog, setShowDeleteDialog] = React.useState(false);
  const [isCreating, setIsCreating] = React.useState(false);
  const [creationType, setCreationType] = React.useState<CreationType>("file");
  const [pendingDelete, setPendingDelete] = React.useState(false);
  const renameInputRef = React.useRef<HTMLInputElement>(null);
  const newItemInputRef = React.useRef<HTMLInputElement>(null);
  // Start closed by default. We'll open on explicit reveal events or user interaction.
  const [isOpen, setIsOpen] = React.useState(false);

  // Listen for explicit reveal events (breadcrumb click or initial mount)
  React.useEffect(() => {
    const handler = (e: Event) => {
      const anyE = e as CustomEvent<{ path: string }>;
      const p = anyE?.detail?.path;
      if (p && isDescendant(p, fileTree.path) && !isOpen) {
        setIsOpen(true);
      }
    };

    window.addEventListener(SIDEBAR_REVEAL_FILE, handler as EventListener);
    return () =>
      window.removeEventListener(SIDEBAR_REVEAL_FILE, handler as EventListener);
  }, [fileTree.path, isOpen]);

  // On initial mount, if activePath indicates a descendant, open this directory.
  React.useEffect(() => {
    if (activePath && isDescendant(activePath, fileTree.path)) {
      setIsOpen(true);
    }
    // run only on initial mount
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const handleRename = () => {
    if (isReadonly) return;
    setIsEditing(true);
  };

  const handleDelete = () => {
    if (isReadonly) return;
    setPendingDelete(true);
    setIsContextMenuOpen(false);
  };

  const handleCreateFile = () => {
    if (isReadonly) return;
    setCreationType("file");
    setIsCreating(true);
    setIsOpen(true);
  };

  const handleCreateFolder = () => {
    if (isReadonly) return;
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
                {isCreating && !isReadonly && (
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
                      <FileTreeNode fileTree={f} activePath={activePath} />
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
          {!isReadonly && (
            <>
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
    </div>
  );
};
