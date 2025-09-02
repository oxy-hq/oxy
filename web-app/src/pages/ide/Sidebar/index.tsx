import useFileTree from "@/hooks/api/files/useFileTree";
import {
  SidebarContent,
  SidebarGroup,
  SidebarGroupLabel,
  SidebarMenu,
  SidebarTrigger,
  useSidebar,
} from "@/components/ui/shadcn/sidebar";
import FileTreeNode from "./FileTreeNode";
import {
  ChevronsLeft,
  ChevronsRight,
  FilePlus,
  FolderPlus,
  Loader2,
  RotateCw,
} from "lucide-react";
import { Button } from "@/components/ui/shadcn/button";
import React from "react";
import NewNode, { CreationType } from "./NewNode";
import { useReadonly } from "@/hooks/useReadonly";

// Reuse a single collator for faster, locale-aware, case-insensitive comparisons
const NAME_COLLATOR = new Intl.Collator(undefined, {
  sensitivity: "base",
  numeric: true,
});

const ignoreFilesRegex = [/^docker-entrypoints/, /^output/, /^\./];

const Sidebar = ({
  sidebarOpen,
  setSidebarOpen,
}: {
  sidebarOpen: boolean;
  setSidebarOpen: (open: boolean) => void;
}) => {
  const { data, refetch, isPending } = useFileTree();
  const { isReadonly } = useReadonly();
  const fileTree = data
    ?.filter((f) => !ignoreFilesRegex.some((r) => f.name.match(r)))
    .sort((a, b) => {
      // Directories first
      if (a.is_dir && !b.is_dir) return -1;
      if (!a.is_dir && b.is_dir) return 1;
      // Locale-aware, case-insensitive, numeric-aware compare using a shared collator
      return NAME_COLLATOR.compare(a.name, b.name);
    });

  const { open } = useSidebar();
  const [isCreating, setIsCreating] = React.useState(false);
  const [creationType, setCreationType] = React.useState<CreationType>("file");
  const handleCreateFile = () => {
    if (isReadonly) return;
    setCreationType("file");
    setIsCreating(true);
  };

  const handleCreateFolder = () => {
    if (isReadonly) return;
    setCreationType("folder");
    setIsCreating(true);
  };

  return (
    <div className="h-full w-full border-r border-l bg-sidebar-background">
      {sidebarOpen && (
        <div className="flex h-full flex-col overflow-hidden">
          <SidebarGroupLabel className="h-auto flex items-center justify-between p-2 border-b border-sidebar-border">
            <div className="flex items-center gap-2">
              {!open && <SidebarTrigger />}
              Files
            </div>

            <div className="flex items-center">
              <Button
                variant="ghost"
                size="sm"
                onClick={handleCreateFile}
                disabled={isReadonly}
                tooltip={isReadonly ? "Read-only mode" : "New File"}
              >
                <FilePlus />
              </Button>

              <Button
                variant="ghost"
                size="sm"
                onClick={handleCreateFolder}
                disabled={isReadonly}
                tooltip={isReadonly ? "Read-only mode" : "New Folder"}
              >
                <FolderPlus />
              </Button>

              <Button
                variant="ghost"
                size="sm"
                onClick={() => refetch()}
                tooltip="Refresh Files"
              >
                <RotateCw />
              </Button>

              <Button
                className="md:hidden"
                variant="ghost"
                size="icon"
                onClick={() => setSidebarOpen(!sidebarOpen)}
                tooltip="Collapse Sidebar"
              >
                <ChevronsLeft />
              </Button>
            </div>
          </SidebarGroupLabel>
          <SidebarContent className="customScrollbar h-full flex-1 overflow-y-auto">
            <SidebarGroup>
              <SidebarMenu>
                {isPending && (
                  <div className="flex items-center justify-center p-2">
                    <Loader2 className="animate-spin h-4 w-4" />
                  </div>
                )}
                {isCreating && !isReadonly && (
                  <NewNode
                    creationType={creationType}
                    onCreated={() => {
                      setIsCreating(false);
                      refetch();
                    }}
                    onCancel={() => setIsCreating(false)}
                  />
                )}
                {fileTree?.map((item) => (
                  <FileTreeNode key={item.path} fileTree={item} />
                ))}
              </SidebarMenu>
            </SidebarGroup>
          </SidebarContent>
        </div>
      )}
      {!sidebarOpen && (
        <Button
          variant="ghost"
          size="icon"
          onClick={() => setSidebarOpen(!sidebarOpen)}
          tooltip="Expand Sidebar"
        >
          <ChevronsRight />
        </Button>
      )}
    </div>
  );
};

export default Sidebar;
