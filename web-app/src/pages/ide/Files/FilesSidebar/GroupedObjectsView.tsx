import { AppWindow, ChevronDown, ChevronRight, Database, Workflow } from "lucide-react";
import React from "react";
import { useNavigate } from "react-router-dom";
import { Badge } from "@/components/ui/shadcn/badge";
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
import useApps from "@/hooks/api/apps/useApps";
import { encodeBase64 } from "@/libs/encoding";
import ROUTES from "@/libs/utils/routes";
import useCurrentOrg from "@/stores/useCurrentOrg";
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
  /** Optional per-file trailing badge (e.g. "Draft" pill for unpublished apps). */
  badgeFor?: (file: FileTreeModel) => React.ReactNode;
}

const CollapsibleGroup: React.FC<CollapsibleGroupProps> = ({
  label,
  files,
  isOpen,
  activePath,
  onToggle,
  onFileClick,
  icon: Icon,
  badgeFor
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
                  <span className='flex-1 truncate'>{getObjectName(file)}</span>
                  {badgeFor?.(file)}
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

const GroupedObjectsView: React.FC<GroupedObjectsViewProps> = ({
  files,
  activePath,
  projectId
}) => {
  const grouped = React.useMemo(() => groupObjectsByType(files), [files]);
  const navigate = useNavigate();
  const orgSlug = useCurrentOrg((s) => s.org?.slug) ?? "";
  const [openGroups, setOpenGroups] = React.useState({
    procedures: true,
    pipelines: true,
    agents: true,
    apps: true,
    tests: true
  });

  const { data: appList } = useApps();
  const publishedByPath = React.useMemo(() => {
    const map = new Map<string, boolean>();
    for (const a of appList ?? []) {
      map.set(a.path, !!a.published);
    }
    return map;
  }, [appList]);
  const appBadge = React.useCallback(
    (file: FileTreeModel) => {
      const published = publishedByPath.get(file.path);
      if (published === undefined || published) return null;
      return (
        <Badge variant='outline' className='font-normal text-[10px] text-muted-foreground'>
          Draft
        </Badge>
      );
    },
    [publishedByPath]
  );

  const toggleGroup = (group: keyof typeof openGroups) => {
    setOpenGroups((prev) => ({ ...prev, [group]: !prev[group] }));
  };

  const handleFileClick = (file: FileTreeModel) => {
    const pathb64 = encodeBase64(file.path);
    navigate(ROUTES.ORG(orgSlug).WORKSPACE(projectId).IDE.FILES.FILE(pathb64));
  };

  return (
    <SidebarMenu className='pb-20'>
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
        label='Pipelines'
        files={grouped.pipelines}
        isOpen={openGroups.pipelines}
        activePath={activePath}
        onToggle={() => toggleGroup("pipelines")}
        onFileClick={handleFileClick}
        icon={Database}
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
        badgeFor={appBadge}
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
