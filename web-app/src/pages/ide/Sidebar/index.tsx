import useFileTree from "@/hooks/api/useFileTree";
import {
  SidebarContent,
  SidebarGroup,
  SidebarGroupLabel,
  SidebarMenu,
  SidebarTrigger,
} from "@/components/ui/shadcn/sidebar";
import FileTreeNode from "./FileTreeNode";
import { ChevronsLeft, ChevronsRight } from "lucide-react";
import { Button } from "@/components/ui/shadcn/button";

const ignoreFilesRegex = [/^docker-entrypoints/, /^output/, /^\./];

const Sidebar = ({
  sidebarOpen,
  setSidebarOpen,
}: {
  sidebarOpen: boolean;
  setSidebarOpen: (open: boolean) => void;
}) => {
  const { data } = useFileTree();
  const fileTree = data
    ?.filter((f) => !ignoreFilesRegex.some((r) => f.name.match(r)))
    .sort((a) => (a.is_dir ? -1 : 1));

  return (
    <div className="h-full w-full border-r border-l">
      {sidebarOpen && (
        <SidebarContent className="customScrollbar h-full">
          <SidebarGroup>
            <SidebarGroupLabel className="flex items-center justify-between">
              <div className="flex items-center gap-2">
                <SidebarTrigger className="md:hidden" />
                Files
              </div>

              <Button
                className="md:hidden"
                variant="ghost"
                size="icon"
                onClick={() => setSidebarOpen(!sidebarOpen)}
              >
                <ChevronsLeft />
              </Button>
            </SidebarGroupLabel>
            <SidebarMenu>
              {fileTree?.map((item) => <FileTreeNode fileTree={item} />)}
            </SidebarMenu>
          </SidebarGroup>
        </SidebarContent>
      )}
      {!sidebarOpen && (
        <Button
          variant="ghost"
          size="icon"
          onClick={() => setSidebarOpen(!sidebarOpen)}
        >
          <ChevronsRight />
        </Button>
      )}
    </div>
  );
};

export default Sidebar;
