import React from "react";
import { useNavigate } from "react-router-dom";
import { ChevronDown, ChevronRight, Workflow, AppWindow } from "lucide-react";
import {
  SidebarMenu,
  SidebarMenuItem,
  SidebarMenuSub,
  SidebarMenuSubItem,
  SidebarMenuSubButton,
  SidebarGroupLabel,
} from "@/components/ui/shadcn/sidebar";
import {
  Collapsible,
  CollapsibleTrigger,
  CollapsibleContent,
} from "@/components/ui/shadcn/collapsible";
import { detectFileType } from "@/utils/fileTypes";
import { FileTreeModel } from "@/types/file";
import ROUTES from "@/libs/utils/routes";
import { getFileTypeIcon, getObjectName, groupObjectsByType } from "./utils";

interface GroupedObjectsViewProps {
  files: FileTreeModel[];
  activePath?: string;
  projectId: string;
}

const GroupedObjectsView: React.FC<GroupedObjectsViewProps> = ({
  files,
  activePath,
  projectId,
}) => {
  const grouped = React.useMemo(() => groupObjectsByType(files), [files]);
  const navigate = useNavigate();
  const [openGroups, setOpenGroups] = React.useState({
    automations: true,
    agents: true,
    apps: true,
    semanticObjects: true,
  });

  const toggleGroup = (group: keyof typeof openGroups) => {
    setOpenGroups((prev) => ({ ...prev, [group]: !prev[group] }));
  };

  const handleFileClick = (file: FileTreeModel) => {
    const pathb64 = btoa(file.path);
    navigate(ROUTES.PROJECT(projectId).IDE.FILES.FILE(pathb64));
  };

  return (
    <SidebarMenu className="pb-20">
      {grouped.semanticObjects.length > 0 && (
        <Collapsible
          open={openGroups.semanticObjects}
          onOpenChange={() => toggleGroup("semanticObjects")}
        >
          <SidebarMenuItem>
            <CollapsibleTrigger asChild>
              <SidebarGroupLabel className="group/label flex justify-between text-muted-foreground hover:text-sidebar-foreground hover:bg-sidebar-accent transition-colors duration-150 ease-in font-semibold">
                <span>Semantic Layer</span>
                {openGroups.semanticObjects ? (
                  <ChevronDown className="transition-transform" />
                ) : (
                  <ChevronRight className="transition-transform" />
                )}
              </SidebarGroupLabel>
            </CollapsibleTrigger>
            <CollapsibleContent>
              <SidebarMenuSub className="border-l-0">
                {grouped.semanticObjects.map((file) => {
                  const fileType = detectFileType(file.path);
                  const Icon = getFileTypeIcon(fileType, file.name);
                  return (
                    <SidebarMenuSubItem key={file.path}>
                      <SidebarMenuSubButton
                        onClick={() => handleFileClick(file)}
                        isActive={activePath === file.path}
                        className="text-muted-foreground hover:text-sidebar-foreground transition-colors duration-150 ease-in"
                      >
                        {Icon && <Icon />}
                        <span>{getObjectName(file)}</span>
                      </SidebarMenuSubButton>
                    </SidebarMenuSubItem>
                  );
                })}
              </SidebarMenuSub>
            </CollapsibleContent>
          </SidebarMenuItem>
        </Collapsible>
      )}

      {grouped.automations.length > 0 && (
        <Collapsible
          open={openGroups.automations}
          onOpenChange={() => toggleGroup("automations")}
        >
          <SidebarMenuItem>
            <CollapsibleTrigger asChild>
              <SidebarGroupLabel className="group/label flex justify-between text-muted-foreground hover:text-sidebar-foreground hover:bg-sidebar-accent transition-colors duration-150 ease-in font-semibold">
                <span>Automations</span>
                {openGroups.automations ? (
                  <ChevronDown className="transition-transform" />
                ) : (
                  <ChevronRight className="transition-transform" />
                )}
              </SidebarGroupLabel>
            </CollapsibleTrigger>
            <CollapsibleContent>
              <SidebarMenuSub className="border-l-0">
                {grouped.automations.map((file) => (
                  <SidebarMenuSubItem key={file.path}>
                    <SidebarMenuSubButton
                      onClick={() => handleFileClick(file)}
                      isActive={activePath === file.path}
                      className="text-muted-foreground hover:text-sidebar-foreground transition-colors duration-150 ease-in"
                    >
                      <Workflow />
                      <span>{getObjectName(file)}</span>
                    </SidebarMenuSubButton>
                  </SidebarMenuSubItem>
                ))}
              </SidebarMenuSub>
            </CollapsibleContent>
          </SidebarMenuItem>
        </Collapsible>
      )}

      {grouped.agents.length > 0 && (
        <Collapsible
          open={openGroups.agents}
          onOpenChange={() => toggleGroup("agents")}
        >
          <SidebarMenuItem>
            <CollapsibleTrigger asChild>
              <SidebarGroupLabel className="group/label flex justify-between text-muted-foreground hover:text-sidebar-foreground hover:bg-sidebar-accent transition-colors duration-150 ease-in font-semibold">
                <span>Agents</span>
                {openGroups.agents ? (
                  <ChevronDown className="transition-transform" />
                ) : (
                  <ChevronRight className="transition-transform" />
                )}
              </SidebarGroupLabel>
            </CollapsibleTrigger>
            <CollapsibleContent>
              <SidebarMenuSub className="border-l-0">
                {grouped.agents.map((file) => {
                  const fileType = detectFileType(file.path);
                  const Icon = getFileTypeIcon(fileType, file.name);
                  return (
                    <SidebarMenuSubItem key={file.path}>
                      <SidebarMenuSubButton
                        onClick={() => handleFileClick(file)}
                        isActive={activePath === file.path}
                        className="text-muted-foreground hover:text-sidebar-foreground transition-colors duration-150 ease-in"
                      >
                        {Icon && <Icon />}
                        <span>{getObjectName(file)}</span>
                      </SidebarMenuSubButton>
                    </SidebarMenuSubItem>
                  );
                })}
              </SidebarMenuSub>
            </CollapsibleContent>
          </SidebarMenuItem>
        </Collapsible>
      )}

      {grouped.apps.length > 0 && (
        <Collapsible
          open={openGroups.apps}
          onOpenChange={() => toggleGroup("apps")}
        >
          <SidebarMenuItem>
            <CollapsibleTrigger asChild>
              <SidebarGroupLabel className="group/label flex justify-between text-muted-foreground hover:text-sidebar-foreground hover:bg-sidebar-accent transition-colors duration-150 ease-in font-semibold">
                <span>Apps</span>
                {openGroups.apps ? (
                  <ChevronDown className="transition-transform" />
                ) : (
                  <ChevronRight className="transition-transform" />
                )}
              </SidebarGroupLabel>
            </CollapsibleTrigger>
            <CollapsibleContent>
              <SidebarMenuSub className="border-l-0">
                {grouped.apps.map((file) => (
                  <SidebarMenuSubItem key={file.path}>
                    <SidebarMenuSubButton
                      onClick={() => handleFileClick(file)}
                      isActive={activePath === file.path}
                      className="text-muted-foreground hover:text-sidebar-foreground transition-colors duration-150 ease-in"
                    >
                      <AppWindow />
                      <span>{getObjectName(file)}</span>
                    </SidebarMenuSubButton>
                  </SidebarMenuSubItem>
                ))}
              </SidebarMenuSub>
            </CollapsibleContent>
          </SidebarMenuItem>
        </Collapsible>
      )}
    </SidebarMenu>
  );
};

export default GroupedObjectsView;
