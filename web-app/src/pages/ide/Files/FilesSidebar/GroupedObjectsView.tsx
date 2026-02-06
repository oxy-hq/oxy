import { AppWindow, ChevronDown, ChevronRight, Workflow } from "lucide-react";
import React from "react";
import { useNavigate } from "react-router-dom";
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger
} from "@/components/ui/shadcn/collapsible";
import {
  SidebarGroupLabel,
  SidebarMenu,
  SidebarMenuItem,
  SidebarMenuSub,
  SidebarMenuSubButton,
  SidebarMenuSubItem
} from "@/components/ui/shadcn/sidebar";
import ROUTES from "@/libs/utils/routes";
import type { FileTreeModel } from "@/types/file";
import { detectFileType } from "@/utils/fileTypes";
import { getFileTypeIcon, getObjectName, groupObjectsByType } from "./utils";

interface GroupedObjectsViewProps {
  files: FileTreeModel[];
  activePath?: string;
  projectId: string;
}

const GroupedObjectsView: React.FC<GroupedObjectsViewProps> = ({
  files,
  activePath,
  projectId
}) => {
  const grouped = React.useMemo(() => groupObjectsByType(files), [files]);
  const navigate = useNavigate();
  const [openGroups, setOpenGroups] = React.useState({
    automations: true,
    agents: true,
    apps: true,
    semanticObjects: true
  });

  const toggleGroup = (group: keyof typeof openGroups) => {
    setOpenGroups((prev) => ({ ...prev, [group]: !prev[group] }));
  };

  const handleFileClick = (file: FileTreeModel) => {
    const pathb64 = btoa(file.path);
    navigate(ROUTES.PROJECT(projectId).IDE.FILES.FILE(pathb64));
  };

  return (
    <SidebarMenu className='pb-20'>
      {grouped.semanticObjects.length > 0 && (
        <Collapsible
          open={openGroups.semanticObjects}
          onOpenChange={() => toggleGroup("semanticObjects")}
        >
          <SidebarMenuItem>
            <CollapsibleTrigger asChild>
              <SidebarGroupLabel className='group/label flex justify-between font-semibold text-muted-foreground transition-colors duration-150 ease-in hover:bg-sidebar-accent hover:text-sidebar-foreground'>
                <span>Semantic Layer</span>
                {openGroups.semanticObjects ? (
                  <ChevronDown className='transition-transform' />
                ) : (
                  <ChevronRight className='transition-transform' />
                )}
              </SidebarGroupLabel>
            </CollapsibleTrigger>
            <CollapsibleContent>
              <SidebarMenuSub className='border-l-0'>
                {grouped.semanticObjects.map((file) => {
                  const fileType = detectFileType(file.path);
                  const Icon = getFileTypeIcon(fileType, file.name);
                  return (
                    <SidebarMenuSubItem key={file.path}>
                      <SidebarMenuSubButton
                        onClick={() => handleFileClick(file)}
                        isActive={activePath === file.path}
                        className='text-muted-foreground transition-colors duration-150 ease-in hover:text-sidebar-foreground'
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
        <Collapsible open={openGroups.automations} onOpenChange={() => toggleGroup("automations")}>
          <SidebarMenuItem>
            <CollapsibleTrigger asChild>
              <SidebarGroupLabel className='group/label flex justify-between font-semibold text-muted-foreground transition-colors duration-150 ease-in hover:bg-sidebar-accent hover:text-sidebar-foreground'>
                <span>Automations</span>
                {openGroups.automations ? (
                  <ChevronDown className='transition-transform' />
                ) : (
                  <ChevronRight className='transition-transform' />
                )}
              </SidebarGroupLabel>
            </CollapsibleTrigger>
            <CollapsibleContent>
              <SidebarMenuSub className='border-l-0'>
                {grouped.automations.map((file) => (
                  <SidebarMenuSubItem key={file.path}>
                    <SidebarMenuSubButton
                      onClick={() => handleFileClick(file)}
                      isActive={activePath === file.path}
                      className='text-muted-foreground transition-colors duration-150 ease-in hover:text-sidebar-foreground'
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
        <Collapsible open={openGroups.agents} onOpenChange={() => toggleGroup("agents")}>
          <SidebarMenuItem>
            <CollapsibleTrigger asChild>
              <SidebarGroupLabel className='group/label flex justify-between font-semibold text-muted-foreground transition-colors duration-150 ease-in hover:bg-sidebar-accent hover:text-sidebar-foreground'>
                <span>Agents</span>
                {openGroups.agents ? (
                  <ChevronDown className='transition-transform' />
                ) : (
                  <ChevronRight className='transition-transform' />
                )}
              </SidebarGroupLabel>
            </CollapsibleTrigger>
            <CollapsibleContent>
              <SidebarMenuSub className='border-l-0'>
                {grouped.agents.map((file) => {
                  const fileType = detectFileType(file.path);
                  const Icon = getFileTypeIcon(fileType, file.name);
                  return (
                    <SidebarMenuSubItem key={file.path}>
                      <SidebarMenuSubButton
                        onClick={() => handleFileClick(file)}
                        isActive={activePath === file.path}
                        className='text-muted-foreground transition-colors duration-150 ease-in hover:text-sidebar-foreground'
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
        <Collapsible open={openGroups.apps} onOpenChange={() => toggleGroup("apps")}>
          <SidebarMenuItem>
            <CollapsibleTrigger asChild>
              <SidebarGroupLabel className='group/label flex justify-between font-semibold text-muted-foreground transition-colors duration-150 ease-in hover:bg-sidebar-accent hover:text-sidebar-foreground'>
                <span>Apps</span>
                {openGroups.apps ? (
                  <ChevronDown className='transition-transform' />
                ) : (
                  <ChevronRight className='transition-transform' />
                )}
              </SidebarGroupLabel>
            </CollapsibleTrigger>
            <CollapsibleContent>
              <SidebarMenuSub className='border-l-0'>
                {grouped.apps.map((file) => (
                  <SidebarMenuSubItem key={file.path}>
                    <SidebarMenuSubButton
                      onClick={() => handleFileClick(file)}
                      isActive={activePath === file.path}
                      className='text-muted-foreground transition-colors duration-150 ease-in hover:text-sidebar-foreground'
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
