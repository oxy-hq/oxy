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

function FileIcon({ file }: { file: FileTreeModel }) {
  const fileType = detectFileType(file.path);
  const Icon = getFileTypeIcon(fileType, file.name);
  return Icon ? <Icon /> : null;
}

interface CollapsibleGroupProps {
  label: string;
  files: FileTreeModel[];
  isOpen: boolean;
  activePath?: string;
  onToggle: () => void;
  onFileClick: (file: FileTreeModel) => void;
  icon?: React.ComponentType;
}

const CollapsibleGroup: React.FC<CollapsibleGroupProps> = ({
  label,
  files,
  isOpen,
  activePath,
  onToggle,
  onFileClick,
  icon: Icon
}) => {
  if (files.length === 0) return null;

  return (
    <Collapsible open={isOpen} onOpenChange={onToggle}>
      <SidebarMenuItem>
        <CollapsibleTrigger asChild>
          <SidebarGroupLabel className='group/label flex justify-between font-semibold text-muted-foreground transition-colors duration-150 ease-in hover:bg-sidebar-accent hover:text-sidebar-foreground'>
            <span>{label}</span>
            {isOpen ? (
              <ChevronDown className='transition-transform' />
            ) : (
              <ChevronRight className='transition-transform' />
            )}
          </SidebarGroupLabel>
        </CollapsibleTrigger>
        <CollapsibleContent>
          <SidebarMenuSub className='border-l-0'>
            {files.map((file) => (
              <SidebarMenuSubItem key={file.path}>
                <SidebarMenuSubButton
                  onClick={() => onFileClick(file)}
                  isActive={activePath === file.path}
                  className='text-muted-foreground transition-colors duration-150 ease-in hover:text-sidebar-foreground'
                >
                  {Icon ? <Icon /> : <FileIcon file={file} />}
                  <span>{getObjectName(file)}</span>
                </SidebarMenuSubButton>
              </SidebarMenuSubItem>
            ))}
          </SidebarMenuSub>
        </CollapsibleContent>
      </SidebarMenuItem>
    </Collapsible>
  );
};

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
    apps: true,
    tests: true
  });

  const { data: lookerIntegrations } = useLookerIntegrations();

  const toggleGroup = (group: keyof typeof openGroups) => {
    setOpenGroups((prev) => ({ ...prev, [group]: !prev[group] }));
  };

  const handleFileClick = (file: FileTreeModel) => {
    const pathb64 = encodeBase64(file.path);
    navigate(ROUTES.WORKSPACE(projectId).IDE.FILES.FILE(pathb64));
  };

  const handleExploreClick = (integrationName: string, model: string, exploreName: string) => {
    navigate(
      ROUTES.WORKSPACE(projectId).IDE.FILES.LOOKER_EXPLORE(integrationName, model, exploreName)
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

      <CollapsibleGroup
        label='Procedures'
        files={grouped.procedures}
        isOpen={openGroups.procedures}
        activePath={activePath}
        onToggle={() => toggleGroup("procedures")}
        onFileClick={handleFileClick}
        icon={Workflow}
      />

      <CollapsibleGroup
        label='Agents'
        files={grouped.agents}
        isOpen={openGroups.agents}
        activePath={activePath}
        onToggle={() => toggleGroup("agents")}
        onFileClick={handleFileClick}
      />

      <CollapsibleGroup
        label='Apps'
        files={grouped.apps}
        isOpen={openGroups.apps}
        activePath={activePath}
        onToggle={() => toggleGroup("apps")}
        onFileClick={handleFileClick}
        icon={AppWindow}
      />

      <CollapsibleGroup
        label='Tests'
        files={grouped.tests}
        isOpen={openGroups.tests}
        activePath={activePath}
        onToggle={() => toggleGroup("tests")}
        onFileClick={handleFileClick}
      />
    </SidebarMenu>
  );
};

export default GroupedObjectsView;
