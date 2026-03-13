import { AppWindow, BookOpen, ChevronDown, ChevronRight, Workflow } from "lucide-react";
import React from "react";
import { useNavigate, useParams } from "react-router-dom";
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
import useLookerIntegrations from "@/hooks/api/integrations/useLookerIntegrations";
import { encodeBase64 } from "@/libs/encoding";
import ROUTES from "@/libs/utils/routes";
import type { FileTreeModel } from "@/types/file";
import { detectFileType } from "@/utils/fileTypes";
import { getFileTypeIcon, getObjectName, groupObjectsByType } from "./utils";

interface GroupedObjectsViewProps {
  files: FileTreeModel[];
  activePath?: string;
  projectId: string;
}

const LookerLogo = () => (
  <img src='/looker.svg' alt='Looker' className='mb-1.5 h-3 w-3 shrink-0 self-start opacity-80' />
);

const GroupedObjectsView: React.FC<GroupedObjectsViewProps> = ({
  files,
  activePath,
  projectId
}) => {
  const grouped = React.useMemo(() => groupObjectsByType(files), [files]);
  const navigate = useNavigate();
  const {
    integrationName: activeIntegration,
    model: activeModel,
    exploreName: activeExplore
  } = useParams<{ integrationName?: string; model?: string; exploreName?: string }>();
  const [openGroups, setOpenGroups] = React.useState({
    semanticObjects: true,
    procedures: true,
    agents: true,
    apps: true
  });

  const { data: lookerIntegrations } = useLookerIntegrations();

  const toggleGroup = (group: keyof typeof openGroups) => {
    setOpenGroups((prev) => ({ ...prev, [group]: !prev[group] }));
  };

  const handleFileClick = (file: FileTreeModel) => {
    const pathb64 = encodeBase64(file.path);
    navigate(ROUTES.PROJECT(projectId).IDE.FILES.FILE(pathb64));
  };

  const handleExploreClick = (integrationName: string, model: string, exploreName: string) => {
    navigate(
      ROUTES.PROJECT(projectId).IDE.FILES.LOOKER_EXPLORE(integrationName, model, exploreName)
    );
  };

  const hasLookerExplores = lookerIntegrations?.some((i) => i.explores.length > 0);
  const multipleIntegrations = (lookerIntegrations?.length ?? 0) > 1;

  return (
    <SidebarMenu className='pb-20'>
      {(grouped.semanticObjects.length > 0 || hasLookerExplores) && (
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
                {lookerIntegrations?.map((integration) =>
                  integration.explores.map((explore) => {
                    const isActive =
                      activeIntegration === integration.name &&
                      activeModel === explore.model &&
                      activeExplore === explore.name;
                    const label = multipleIntegrations
                      ? `${integration.name} / ${explore.name}`
                      : explore.name;
                    return (
                      <SidebarMenuSubItem
                        key={`${integration.name}/${explore.model}/${explore.name}`}
                      >
                        <SidebarMenuSubButton
                          onClick={() =>
                            handleExploreClick(integration.name, explore.model, explore.name)
                          }
                          isActive={isActive}
                          className='text-muted-foreground transition-colors duration-150 ease-in hover:text-sidebar-foreground'
                        >
                          <BookOpen className='shrink-0' />
                          <span className='truncate'>{label}</span>
                          <LookerLogo />
                        </SidebarMenuSubButton>
                      </SidebarMenuSubItem>
                    );
                  })
                )}
              </SidebarMenuSub>
            </CollapsibleContent>
          </SidebarMenuItem>
        </Collapsible>
      )}

      {grouped.procedures.length > 0 && (
        <Collapsible open={openGroups.procedures} onOpenChange={() => toggleGroup("procedures")}>
          <SidebarMenuItem>
            <CollapsibleTrigger asChild>
              <SidebarGroupLabel className='group/label flex justify-between font-semibold text-muted-foreground transition-colors duration-150 ease-in hover:bg-sidebar-accent hover:text-sidebar-foreground'>
                <span>Procedures</span>
                {openGroups.procedures ? (
                  <ChevronDown className='transition-transform' />
                ) : (
                  <ChevronRight className='transition-transform' />
                )}
              </SidebarGroupLabel>
            </CollapsibleTrigger>
            <CollapsibleContent>
              <SidebarMenuSub className='border-l-0'>
                {grouped.procedures.map((file) => (
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
