import {
  SidebarMenuButton,
  SidebarMenuItem,
  SidebarMenuSub,
  SidebarMenuSubButton,
  SidebarMenuSubItem,
} from "@/components/ui/shadcn/sidebar";
import { FileTreeModel } from "@/types/file";
import { Link, useLocation } from "react-router-dom";
import { ChevronRight, File } from "lucide-react";
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@/components/ui/shadcn/collapsible";

const FileTreeNode = ({ fileTree }: { fileTree: FileTreeModel }) => {
  if (fileTree.is_dir) {
    return <DirNode fileTree={fileTree} />;
  }

  return <FileNode fileTree={fileTree} />;
};

export default FileTreeNode;

const DirNode = ({ fileTree }: { fileTree: FileTreeModel }) => {
  return (
    <Collapsible defaultOpen className="group/collapsible">
      <SidebarMenuItem key={fileTree.name}>
        <CollapsibleTrigger asChild>
          <SidebarMenuButton>
            <ChevronRight className="transition-transform duration-200 group-data-[state=open]/collapsible:rotate-90" />
            <span>{fileTree.name}</span>
          </SidebarMenuButton>
        </CollapsibleTrigger>

        {fileTree.children.length > 0 && (
          <CollapsibleContent>
            <SidebarMenuSub>
              {fileTree.children?.map((f) => {
                return (
                  <SidebarMenuSubItem key={f.path}>
                    <SidebarMenuSubButton asChild>
                      <FileTreeNode fileTree={f} />
                    </SidebarMenuSubButton>
                  </SidebarMenuSubItem>
                );
              })}
            </SidebarMenuSub>
          </CollapsibleContent>
        )}
      </SidebarMenuItem>
    </Collapsible>
  );
};

const FileNode = ({ fileTree }: { fileTree: FileTreeModel }) => {
  const { pathname } = useLocation();
  const isActive = pathname === `/ide/${btoa(fileTree.path)}`;

  return (
    <SidebarMenuItem key={fileTree.name}>
      <SidebarMenuButton asChild isActive={isActive}>
        <Link to={`/ide/${btoa(fileTree.path)}`}>
          <File />
          <span>{fileTree.name}</span>
        </Link>
      </SidebarMenuButton>
    </SidebarMenuItem>
  );
};
