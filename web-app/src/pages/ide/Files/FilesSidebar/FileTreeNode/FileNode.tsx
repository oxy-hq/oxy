import {
  AppWindow,
  BookOpen,
  Bot,
  Braces,
  Eye,
  File,
  FileCode,
  Network,
  Pencil,
  Table,
  Trash2,
  Workflow
} from "lucide-react";
import React from "react";
import { Link, useLocation } from "react-router-dom";
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuTrigger
} from "@/components/ui/shadcn/context-menu";
import { SidebarMenuButton, SidebarMenuItem } from "@/components/ui/shadcn/sidebar";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { cn } from "@/libs/shadcn/utils";
import ROUTES from "@/libs/utils/routes";
import type { FileTreeModel } from "@/types/file";
import { detectFileType, FileType } from "@/utils/fileTypes";
import { SIDEBAR_REVEAL_FILE } from "..";
import AlertDeleteDialog from "./AlertDeleteDialog";
import RenameNode from "./RenameNode";

// Helper to get icon for file type
const getFileIcon = (path: string) => {
  const fileType = detectFileType(path);
  switch (fileType) {
    case FileType.WORKFLOW:
    case FileType.AUTOMATION:
      return Workflow;
    case FileType.AGENT:
      return Bot;
    case FileType.AGENTIC_WORKFLOW:
      return Network;
    case FileType.APP:
      return AppWindow;
    case FileType.VIEW:
      return Eye;
    case FileType.TOPIC:
      return BookOpen;
    case FileType.SQL:
      return FileCode;
    default:
      // Check for JSON and CSV files
      if (path.toLowerCase().endsWith(".json")) {
        return Braces;
      }
      if (path.toLowerCase().endsWith(".csv")) {
        return Table;
      }
      return File;
  }
};

const FileNode = ({ fileTree, activePath }: { fileTree: FileTreeModel; activePath?: string }) => {
  const { project, isReadOnly } = useCurrentProjectBranch();
  const projectId = project.id;

  const { pathname } = useLocation();
  const FileIcon = getFileIcon(fileTree.path);
  const isActive = activePath
    ? activePath === fileTree.path
    : pathname === ROUTES.PROJECT(projectId || "").IDE.FILES.FILE(btoa(fileTree.path));

  const [isContextMenuOpen, setIsContextMenuOpen] = React.useState(false);
  const [isEditing, setIsEditing] = React.useState(false);
  const inputRef = React.useRef<HTMLInputElement>(null);
  const [showDeleteDialog, setShowDeleteDialog] = React.useState(false);
  const [pendingDelete, setPendingDelete] = React.useState(false);

  const itemRef = React.useRef<HTMLLIElement | null>(null);
  const handleRename = () => {
    if (isReadOnly) return;
    setIsEditing(true);
  };

  const handleDelete = () => {
    if (isReadOnly) return;
    setPendingDelete(true);
    setIsContextMenuOpen(false);
  };

  const fileUri = ROUTES.PROJECT(projectId).IDE.FILES.FILE(btoa(fileTree.path));
  // Scroll into view when this file becomes active
  React.useEffect(() => {
    if (isActive && itemRef.current) {
      try {
        itemRef.current.scrollIntoView({
          block: "nearest",
          behavior: "smooth"
        });
      } catch {
        itemRef.current.scrollIntoView({ block: "nearest" });
      }
    }
  }, [isActive]);

  // Listen for explicit reveal event to scroll this file into view
  React.useEffect(() => {
    const handler = (e: Event) => {
      const anyE = e as CustomEvent<{ path: string }>;
      const p = anyE?.detail?.path;
      if (p && p === fileTree.path && itemRef.current) {
        try {
          itemRef.current.scrollIntoView({
            block: "nearest",
            behavior: "smooth"
          });
        } catch {
          itemRef.current.scrollIntoView({ block: "nearest" });
        }
      }
    };

    window.addEventListener(SIDEBAR_REVEAL_FILE, handler as EventListener);
    return () => window.removeEventListener(SIDEBAR_REVEAL_FILE, handler as EventListener);
  }, [fileTree.path]);

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
          <SidebarMenuItem ref={itemRef} key={fileTree.name}>
            <SidebarMenuButton
              asChild
              isActive={isActive}
              className={cn(
                "h-6 overflow-visible py-0.5 text-muted-foreground transition-colors duration-150 ease-in hover:text-sidebar-foreground",
                isContextMenuOpen ? "border border-border" : ""
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
                <Link to={fileUri}>
                  <FileIcon />
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
          {!isReadOnly && (
            <>
              <ContextMenuItem className='cursor-pointer' onClick={handleRename}>
                <Pencil className='mr-2 h-4 w-4' />
                <span>Rename</span>
              </ContextMenuItem>
              <ContextMenuItem className='cursor-pointer text-red-600' onClick={handleDelete}>
                <Trash2 className='mr-2 h-4 w-4' />
                <span>Delete</span>
              </ContextMenuItem>
            </>
          )}
          {isReadOnly && (
            <ContextMenuItem disabled>
              <span className='text-muted-foreground'>Read-only mode</span>
            </ContextMenuItem>
          )}
        </ContextMenuContent>
      </ContextMenu>
    </>
  );
};
export default FileNode;
