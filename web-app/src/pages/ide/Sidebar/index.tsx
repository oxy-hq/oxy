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
  RotateCw,
} from "lucide-react";
import { Button } from "@/components/ui/shadcn/button";
import React from "react";
import NewNode, { CreationType } from "./NewNode";

const ignoreFilesRegex = [/^docker-entrypoints/, /^output/, /^\./];

const Sidebar = ({
  sidebarOpen,
  setSidebarOpen,
}: {
  sidebarOpen: boolean;
  setSidebarOpen: (open: boolean) => void;
}) => {
  const { data, refetch } = useFileTree();
  const fileTree = data
    ?.filter((f) => !ignoreFilesRegex.some((r) => f.name.match(r)))
    .sort((a) => (a.is_dir ? -1 : 1));

  const { open } = useSidebar();
  const [isCreating, setIsCreating] = React.useState(false);
  const [creationType, setCreationType] = React.useState<CreationType>("file");
  const handleCreateFile = () => {
    setCreationType("file");
    setIsCreating(true);
  };

  const handleCreateFolder = () => {
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
                tooltip="New File"
              >
                <FilePlus />
              </Button>

              <Button
                variant="ghost"
                size="sm"
                onClick={handleCreateFolder}
                tooltip="New Folder"
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
                {isCreating && (
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
