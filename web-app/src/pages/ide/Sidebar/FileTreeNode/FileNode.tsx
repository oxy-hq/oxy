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
import {
  Pencil,
  Trash2,
  File,
  Workflow,
  AppWindow,
  Eye,
  BookOpen,
  Bot,
  Network,
  FileCode,
  Braces,
  Table,
} from "lucide-react";
import React from "react";
import { detectFileType, FileType } from "@/utils/fileTypes";
import { SIDEBAR_REVEAL_FILE } from "../events";
import { useLocation, Link } from "react-router-dom";
import AlertDeleteDialog from "../AlertDeleteDialog";
import RenameNode from "../RenameNode";
import { cn } from "@/libs/shadcn/utils";
import ROUTES from "@/libs/utils/routes";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";

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

const FileNode = ({
  fileTree,
  activePath,
}: {
  fileTree: FileTreeModel;
  activePath?: string;
}) => {
  const { project, isReadOnly } = useCurrentProjectBranch();
  const projectId = project.id;

  const { pathname } = useLocation();
  const FileIcon = getFileIcon(fileTree.path);
  const isActive = activePath
    ? activePath === fileTree.path
    : pathname ===
      ROUTES.PROJECT(projectId || "").IDE.FILE(btoa(fileTree.path));

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

  const fileUri = ROUTES.PROJECT(projectId).IDE.FILE(btoa(fileTree.path));
  // Scroll into view when this file becomes active
  React.useEffect(() => {
    if (isActive && itemRef.current) {
      try {
        itemRef.current.scrollIntoView({
          block: "nearest",
          behavior: "smooth",
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
            behavior: "smooth",
          });
        } catch {
          itemRef.current.scrollIntoView({ block: "nearest" });
        }
      }
    };

    window.addEventListener(SIDEBAR_REVEAL_FILE, handler as EventListener);
    return () =>
      window.removeEventListener(SIDEBAR_REVEAL_FILE, handler as EventListener);
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
                "overflow-visible text-muted-foreground hover:text-sidebar-foreground transition-colors duration-150 ease-in h-6 py-0.5",
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
          {isReadOnly && (
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
