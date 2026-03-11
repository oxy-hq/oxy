import { ChevronDown, ChevronRight } from "lucide-react";
import React from "react";
import { useNavigate } from "react-router-dom";
import {
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
  SidebarMenuSub,
  SidebarMenuSubButton,
  SidebarMenuSubItem
} from "@/components/ui/shadcn/sidebar";
import { encodeBase64 } from "@/libs/encoding";
import ROUTES from "@/libs/utils/routes";
import type { FileTreeModel } from "@/types/file";
import { detectFileType } from "@/utils/fileTypes";
import { getFileTypeIcon, getObjectName, groupObjectsByType } from "./utils";

type GroupKey = "semanticObjects" | "procedures" | "agents" | "apps";

const GROUP_CONFIG: { key: GroupKey; label: string }[] = [
  { key: "semanticObjects", label: "Semantic Layer" },
  { key: "procedures", label: "Procedures" },
  { key: "agents", label: "Agents" },
  { key: "apps", label: "Apps" }
];

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
}

const CollapsibleGroup: React.FC<CollapsibleGroupProps> = ({
  label,
  files,
  isOpen,
  activePath,
  onToggle,
  onFileClick
}) => {
  if (files.length === 0) return null;

  const Chevron = isOpen ? ChevronDown : ChevronRight;

  return (
    <SidebarMenuItem>
      <SidebarMenuButton
        onClick={onToggle}
        className='text-sidebar-foreground hover:bg-sidebar-accent'
      >
        <Chevron className='h-4 w-4' />
        <span>{label}</span>
      </SidebarMenuButton>

      {isOpen && (
        <SidebarMenuSub className='ml-[15px]'>
          {files.map((file) => (
            <SidebarMenuSubItem key={file.path}>
              <SidebarMenuSubButton
                className='pointer'
                onClick={() => onFileClick(file)}
                isActive={activePath === file.path}
              >
                <FileIcon file={file} />
                <span>{getObjectName(file)}</span>
              </SidebarMenuSubButton>
            </SidebarMenuSubItem>
          ))}
        </SidebarMenuSub>
      )}
    </SidebarMenuItem>
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
  const [openGroups, setOpenGroups] = React.useState<Record<GroupKey, boolean>>({
    semanticObjects: true,
    procedures: true,
    agents: true,
    apps: true
  });

  const handleFileClick = (file: FileTreeModel) => {
    const pathb64 = encodeBase64(file.path);
    navigate(ROUTES.PROJECT(projectId).IDE.FILES.FILE(pathb64));
  };

  return (
    <SidebarMenu className='pb-20'>
      {GROUP_CONFIG.map(({ key, label }) => (
        <CollapsibleGroup
          key={key}
          label={label}
          files={grouped[key]}
          isOpen={openGroups[key]}
          activePath={activePath}
          onToggle={() => setOpenGroups((prev) => ({ ...prev, [key]: !prev[key] }))}
          onFileClick={handleFileClick}
        />
      ))}
    </SidebarMenu>
  );
};

export default GroupedObjectsView;
