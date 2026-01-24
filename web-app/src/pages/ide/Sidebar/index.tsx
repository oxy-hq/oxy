import useFileTree from "@/hooks/api/files/useFileTree";
import {
  SidebarContent,
  SidebarGroup,
  SidebarGroupLabel,
  SidebarMenu,
  SidebarMenuItem,
  SidebarMenuSub,
  SidebarMenuSubItem,
  SidebarMenuSubButton,
} from "@/components/ui/shadcn/sidebar";
import {
  Collapsible,
  CollapsibleTrigger,
  CollapsibleContent,
} from "@/components/ui/shadcn/collapsible";
import FileTreeNode from "./FileTreeNode";
import NewObjectButton from "./NewObjectButton";
import {
  ChevronsLeft,
  ChevronsRight,
  ChevronDown,
  ChevronRight,
  FilePlus,
  FolderPlus,
  Loader2,
  RotateCw,
  Workflow,
  AppWindow,
  Eye,
  BookOpen,
  Folder,
  Layers2,
  Bot,
  Network,
  FileCode,
  Braces,
  Table,
  Activity,
  Zap,
  Server,
  BarChart3,
  LucideActivity,
  Database,
  FileText,
  Settings,
} from "lucide-react";
import { Button } from "@/components/ui/shadcn/button";
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/shadcn/tabs";
import { cn } from "@/libs/shadcn/utils";
import React from "react";
import { Link, useLocation, useNavigate } from "react-router-dom";
import NewNode, { CreationType } from "./NewNode";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { useAuth } from "@/contexts/AuthContext";
import { SIDEBAR_REVEAL_FILE } from "./events";
import { detectFileType, FileType } from "@/utils/fileTypes";
import { FileTreeModel } from "@/types/file";
import ROUTES from "@/libs/utils/routes";
import { FilesSubViewMode } from "..";

// Reuse a single collator for faster, locale-aware, case-insensitive comparisons
const NAME_COLLATOR = new Intl.Collator(undefined, {
  sensitivity: "base",
  numeric: true,
});

enum SidebarViewMode {
  FILES = "files",
  OBSERVABILITY = "observability",
  SETTINGS = "settings",
}

const ignoreFilesRegex = [/^docker-entrypoints/, /^output/, /^\./];

// Object file types that should appear in Objects view
const OBJECT_FILE_TYPES = [
  FileType.WORKFLOW,
  FileType.AUTOMATION,
  FileType.AGENT,
  FileType.AGENTIC_WORKFLOW,
  FileType.APP,
  FileType.VIEW,
  FileType.TOPIC,
];

// Helper to check if a file is an object type
const isObjectFile = (file: FileTreeModel): boolean => {
  if (file.is_dir) return false;
  const fileType = detectFileType(file.path);
  return OBJECT_FILE_TYPES.includes(fileType);
};

// Helper to get clean object name (without extension)
const getObjectName = (file: FileTreeModel): string => {
  const fileName = file.name;
  // Remove file type extensions
  return fileName
    .replace(/\.(workflow|automation|agent|aw|app|view|topic)\.(yml|yaml)$/, "")
    .replace(/\.(yml|yaml)$/, "");
};

// Helper to get icon for file type
const getFileTypeIcon = (fileType: FileType, fileName?: string) => {
  switch (fileType) {
    case FileType.WORKFLOW:
    case FileType.AUTOMATION:
      return Workflow;
    case FileType.AGENT:
      return Bot;
    case FileType.AGENTIC_WORKFLOW:
      return Network;
    case FileType.APP:
      return AppWindow;
    case FileType.VIEW:
      return Eye;
    case FileType.TOPIC:
      return BookOpen;
    case FileType.SQL:
      return FileCode;
    default:
      // Check for JSON and CSV files
      if (fileName?.toLowerCase().endsWith(".json")) {
        return Braces;
      }
      if (fileName?.toLowerCase().endsWith(".csv")) {
        return Table;
      }
      return null;
  }
};

// Group objects by type
interface GroupedObjects {
  automations: FileTreeModel[];
  agents: FileTreeModel[];
  apps: FileTreeModel[];
  semanticObjects: FileTreeModel[];
}

const groupObjectsByType = (files: FileTreeModel[]): GroupedObjects => {
  const groups: GroupedObjects = {
    automations: [],
    agents: [],
    apps: [],
    semanticObjects: [],
  };

  files.forEach((file) => {
    if (file.is_dir) return;
    const fileType = detectFileType(file.path);

    switch (fileType) {
      case FileType.WORKFLOW:
      case FileType.AUTOMATION:
      case FileType.AGENTIC_WORKFLOW:
        groups.automations.push(file);
        break;
      case FileType.AGENT:
        groups.agents.push(file);
        break;
      case FileType.APP:
        groups.apps.push(file);
        break;
      case FileType.VIEW:
      case FileType.TOPIC:
        groups.semanticObjects.push(file);
        break;
    }
  });

  // Sort each group alphabetically by name
  groups.automations.sort((a, b) => NAME_COLLATOR.compare(a.name, b.name));
  groups.agents.sort((a, b) => NAME_COLLATOR.compare(a.name, b.name));
  groups.apps.sort((a, b) => NAME_COLLATOR.compare(a.name, b.name));
  groups.semanticObjects.sort((a, b) => NAME_COLLATOR.compare(a.name, b.name));

  return groups;
};

// Helper to build a set of directory paths that contain objects
// This traverses the tree once and marks parent directories bottom-up
const buildDirectoriesWithObjects = (
  allFiles: FileTreeModel[],
): Set<string> => {
  const directoriesWithObjects = new Set<string>();

  // First pass: mark directories that directly contain object files
  allFiles.forEach((file) => {
    if (isObjectFile(file)) {
      // Mark all parent directories
      let currentPath = file.path;
      const lastSlashIndex = currentPath.lastIndexOf("/");
      while (lastSlashIndex > 0) {
        currentPath = currentPath.substring(0, lastSlashIndex);
        directoriesWithObjects.add(currentPath);
        const nextSlashIndex = currentPath.lastIndexOf("/");
        if (nextSlashIndex === -1) break;
      }
    }
  });

  return directoriesWithObjects;
};

// Helper to get all object files from the full file list (recursively searches all paths)
const getAllObjectFiles = (allFiles: FileTreeModel[]): FileTreeModel[] => {
  const objectFiles: FileTreeModel[] = [];

  const traverse = (files: FileTreeModel[]) => {
    files.forEach((file) => {
      if (isObjectFile(file)) {
        objectFiles.push(file);
      }
      // Recursively traverse children directories
      if (file.is_dir && file.children) {
        traverse(file.children);
      }
    });
  };

  traverse(allFiles);
  return objectFiles;
};

// Component for rendering grouped objects
const GroupedObjectsView = ({
  files,
  activePath,
  projectId,
}: {
  files: FileTreeModel[];
  activePath?: string;
  projectId: string;
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
    navigate(ROUTES.PROJECT(projectId).IDE.FILE(pathb64));
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

// Observability section component for IDE sidebar
const ObservabilitySection = ({ projectId }: { projectId: string }) => {
  const location = useLocation();

  return (
    <>
      <SidebarMenuItem>
        <SidebarMenuSubButton
          asChild
          isActive={
            location.pathname ===
            ROUTES.PROJECT(projectId).IDE.OBSERVABILITY.TRACES
          }
          className="text-muted-foreground hover:text-sidebar-foreground transition-colors duration-150 ease-in"
        >
          <Link to={ROUTES.PROJECT(projectId).IDE.OBSERVABILITY.TRACES}>
            <Zap />
            <span>Traces</span>
          </Link>
        </SidebarMenuSubButton>
      </SidebarMenuItem>
      <SidebarMenuItem>
        <SidebarMenuSubButton
          asChild
          isActive={
            location.pathname ===
            ROUTES.PROJECT(projectId).IDE.OBSERVABILITY.CLUSTERS
          }
          className="text-muted-foreground hover:text-sidebar-foreground transition-colors duration-150 ease-in"
        >
          <Link to={ROUTES.PROJECT(projectId).IDE.OBSERVABILITY.CLUSTERS}>
            <Server />
            <span>Clusters</span>
          </Link>
        </SidebarMenuSubButton>
      </SidebarMenuItem>
      <SidebarMenuItem>
        <SidebarMenuSubButton
          asChild
          isActive={
            location.pathname ===
            ROUTES.PROJECT(projectId).IDE.OBSERVABILITY.METRICS
          }
          className="text-muted-foreground hover:text-sidebar-foreground transition-colors duration-150 ease-in"
        >
          <Link to={ROUTES.PROJECT(projectId).IDE.OBSERVABILITY.METRICS}>
            <BarChart3 />
            <span>Metrics</span>
          </Link>
        </SidebarMenuSubButton>
      </SidebarMenuItem>
      <SidebarMenuItem>
        <SidebarMenuSubButton
          asChild
          isActive={
            location.pathname ===
            ROUTES.PROJECT(projectId).IDE.OBSERVABILITY.EXECUTION_ANALYTICS
          }
          className="text-muted-foreground hover:text-sidebar-foreground transition-colors duration-150 ease-in"
        >
          <Link
            to={ROUTES.PROJECT(projectId).IDE.OBSERVABILITY.EXECUTION_ANALYTICS}
          >
            <LucideActivity />
            <span>Execution Analytics</span>
          </Link>
        </SidebarMenuSubButton>
      </SidebarMenuItem>
    </>
  );
};

// Settings section component for IDE sidebar (Databases and Activity Logs)
const SettingsSection = ({ projectId }: { projectId: string }) => {
  const location = useLocation();

  return (
    <>
      <SidebarMenuItem>
        <SidebarMenuSubButton
          asChild
          isActive={
            location.pathname === ROUTES.PROJECT(projectId).IDE.DATABASES
          }
          className="text-muted-foreground hover:text-sidebar-foreground transition-colors duration-150 ease-in"
        >
          <Link to={ROUTES.PROJECT(projectId).IDE.DATABASES}>
            <Database />
            <span>Databases</span>
          </Link>
        </SidebarMenuSubButton>
      </SidebarMenuItem>
      <SidebarMenuItem>
        <SidebarMenuSubButton
          asChild
          isActive={
            location.pathname === ROUTES.PROJECT(projectId).IDE.ACTIVITY_LOGS
          }
          className="text-muted-foreground hover:text-sidebar-foreground transition-colors duration-150 ease-in"
        >
          <Link to={ROUTES.PROJECT(projectId).IDE.ACTIVITY_LOGS}>
            <FileText />
            <span>Activity Logs</span>
          </Link>
        </SidebarMenuSubButton>
      </SidebarMenuItem>
    </>
  );
};

const Sidebar = ({
  sidebarOpen,
  setSidebarOpen,
  filesSubViewMode,
  setFilesSubViewMode,
}: {
  sidebarOpen: boolean;
  setSidebarOpen: (open: boolean) => void;
  filesSubViewMode: FilesSubViewMode;
  setFilesSubViewMode: (mode: FilesSubViewMode) => void;
}) => {
  const { authConfig } = useAuth();
  const { isReadOnly, project } = useCurrentProjectBranch();
  const projectId = project.id;
  const [viewMode, setViewMode] = React.useState<SidebarViewMode>(
    SidebarViewMode.FILES,
  );

  const { data, refetch, isPending } = useFileTree();

  const fileTree = React.useMemo(() => {
    const filtered = data?.filter(
      (f) => !ignoreFilesRegex.some((r) => f.name.match(r)),
    );

    if (!filtered) return undefined;

    // In objects mode, filter to show only objects and directories that contain objects
    if (filesSubViewMode === FilesSubViewMode.OBJECTS) {
      // Pre-compute which directories contain objects for O(n) lookup
      const directoriesWithObjects = buildDirectoriesWithObjects(filtered);

      return filtered
        .filter((f) => {
          if (isObjectFile(f)) return true;
          if (f.is_dir) return directoriesWithObjects.has(f.path);
          return false;
        })
        .sort((a, b) => {
          // In objects mode, sort objects before directories
          if (!a.is_dir && b.is_dir) return -1;
          if (a.is_dir && !b.is_dir) return 1;
          // Within same type, sort alphabetically
          return NAME_COLLATOR.compare(a.name, b.name);
        });
    }

    // In files mode, show everything (original behavior)
    return filtered.sort((a, b) => {
      // Directories first
      if (a.is_dir && !b.is_dir) return -1;
      if (!a.is_dir && b.is_dir) return 1;
      // Locale-aware, case-insensitive, numeric-aware compare using a shared collator
      return NAME_COLLATOR.compare(a.name, b.name);
    });
  }, [data, filesSubViewMode]);

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

  const { pathname } = useLocation();
  const activePath = React.useMemo(() => {
    try {
      const match = pathname.match(/^\/ide\/(.+)/);
      if (match?.[1]) return atob(match[1]);
    } catch {
      // ignore
    }
    return undefined;
  }, [pathname]);

  // On initial mount, if there's an activePath, dispatch an event to reveal it
  React.useEffect(() => {
    if (activePath) {
      try {
        window.dispatchEvent(
          new CustomEvent(SIDEBAR_REVEAL_FILE, {
            detail: { path: activePath },
          }),
        );
      } catch {
        // ignore
      }
    }
    // only on mount
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  return (
    <div className="h-full w-full border-r bg-sidebar-background flex">
      {/* Vertical Icon Bar */}
      <div className="flex flex-col items-center py-2 px-1 border-r border-sidebar-border bg-sidebar-background gap-1">
        <Button
          variant="ghost"
          size="icon"
          onClick={() => setViewMode(SidebarViewMode.FILES)}
          tooltip={{ content: "Files", side: "right" }}
          className={cn(
            "h-8 w-8",
            viewMode === SidebarViewMode.FILES &&
              "bg-sidebar-accent text-sidebar-accent-foreground",
          )}
        >
          <Folder className="h-4 w-4" />
        </Button>

        {authConfig.enterprise && (
          <Button
            variant="ghost"
            size="icon"
            onClick={() => setViewMode(SidebarViewMode.OBSERVABILITY)}
            tooltip={{ content: "Observability", side: "right" }}
            className={cn(
              "h-8 w-8",
              viewMode === SidebarViewMode.OBSERVABILITY &&
                "bg-sidebar-accent text-sidebar-accent-foreground",
            )}
          >
            <Activity className="h-4 w-4" />
          </Button>
        )}

        <Button
          variant="ghost"
          size="icon"
          onClick={() => setViewMode(SidebarViewMode.SETTINGS)}
          tooltip={{ content: "Settings", side: "right" }}
          className={cn(
            "h-8 w-8",
            viewMode === SidebarViewMode.SETTINGS &&
              "bg-sidebar-accent text-sidebar-accent-foreground",
          )}
        >
          <Settings className="h-4 w-4" />
        </Button>
      </div>

      {/* Main Content Area */}
      {sidebarOpen && (
        <div className="flex h-full flex-1 flex-col overflow-hidden">
          {/* Files View */}
          {viewMode === SidebarViewMode.FILES && (
            <>
              <SidebarGroupLabel className="h-auto flex items-center justify-between px-2 py-1 border-b border-sidebar-border rounded-none">
                <Tabs
                  value={filesSubViewMode}
                  onValueChange={(value: string) => {
                    if (
                      value === FilesSubViewMode.FILES ||
                      value === FilesSubViewMode.OBJECTS
                    ) {
                      setFilesSubViewMode(value as FilesSubViewMode);
                    }
                  }}
                >
                  <TabsList className="h-8">
                    <TabsTrigger
                      value={FilesSubViewMode.OBJECTS}
                      className="h-6 px-2 gap-1.5"
                      aria-label="Objects view"
                    >
                      <Layers2 className="w-4 h-4" />
                      <span className="text-xs">Objects</span>
                    </TabsTrigger>
                    <TabsTrigger
                      value={FilesSubViewMode.FILES}
                      className="h-6 px-2 gap-1.5"
                      aria-label="Files view"
                    >
                      <Folder className="w-4 h-4" />
                      <span className="text-xs">Files</span>
                    </TabsTrigger>
                  </TabsList>
                </Tabs>

                <div className="flex items-center gap-0.5">
                  {filesSubViewMode === FilesSubViewMode.OBJECTS && (
                    <NewObjectButton disabled={!!isReadOnly} />
                  )}

                  {filesSubViewMode === FilesSubViewMode.FILES && (
                    <>
                      <Button
                        variant="ghost"
                        size="sm"
                        onClick={handleCreateFile}
                        disabled={!!isReadOnly}
                        tooltip={isReadOnly ? "Read-only mode" : "New File"}
                      >
                        <FilePlus />
                      </Button>

                      <Button
                        variant="ghost"
                        size="sm"
                        onClick={handleCreateFolder}
                        disabled={!!isReadOnly}
                        tooltip={isReadOnly ? "Read-only mode" : "New Folder"}
                      >
                        <FolderPlus />
                      </Button>
                    </>
                  )}

                  <Button
                    variant="ghost"
                    size="sm"
                    onClick={() => refetch()}
                    tooltip="Refresh"
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
                <SidebarGroup className="pt-2">
                  {isPending && (
                    <div className="flex items-center justify-center p-2">
                      <Loader2 className="animate-spin h-4 w-4" />
                    </div>
                  )}

                  {!isPending &&
                    filesSubViewMode === FilesSubViewMode.OBJECTS &&
                    data && (
                      <GroupedObjectsView
                        files={getAllObjectFiles(
                          data.filter(
                            (f) =>
                              !ignoreFilesRegex.some((r) => f.name.match(r)),
                          ),
                        )}
                        activePath={activePath}
                        projectId={projectId}
                      />
                    )}

                  {!isPending &&
                    filesSubViewMode === FilesSubViewMode.FILES && (
                      <SidebarMenu className="pb-20">
                        {isCreating && !isReadOnly && (
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
                          <FileTreeNode
                            key={item.path}
                            fileTree={item}
                            activePath={activePath}
                          />
                        ))}
                      </SidebarMenu>
                    )}
                </SidebarGroup>
              </SidebarContent>
            </>
          )}

          {/* Observability View - Only visible in enterprise mode */}
          {authConfig.enterprise &&
            viewMode === SidebarViewMode.OBSERVABILITY && (
              <>
                <SidebarGroupLabel className="h-auto flex items-center justify-between px-2 py-1 border-b border-sidebar-border rounded-none">
                  <span className="text-sm font-semibold">Observability</span>
                  <Button
                    className="md:hidden"
                    variant="ghost"
                    size="icon"
                    onClick={() => setSidebarOpen(!sidebarOpen)}
                    tooltip="Collapse Sidebar"
                  >
                    <ChevronsLeft />
                  </Button>
                </SidebarGroupLabel>
                <SidebarContent className="customScrollbar h-full flex-1 overflow-y-auto">
                  <SidebarGroup className="pt-2">
                    <SidebarMenu>
                      <ObservabilitySection projectId={projectId} />
                    </SidebarMenu>
                  </SidebarGroup>
                </SidebarContent>
              </>
            )}

          {/* Settings View - Databases and Activity Logs */}
          {viewMode === SidebarViewMode.SETTINGS && (
            <>
              <SidebarGroupLabel className="h-auto flex items-center justify-between px-2 py-1 border-b border-sidebar-border rounded-none">
                <span className="text-sm font-semibold">Settings</span>
                <Button
                  className="md:hidden"
                  variant="ghost"
                  size="icon"
                  onClick={() => setSidebarOpen(!sidebarOpen)}
                  tooltip="Collapse Sidebar"
                >
                  <ChevronsLeft />
                </Button>
              </SidebarGroupLabel>
              <SidebarContent className="customScrollbar h-full flex-1 overflow-y-auto">
                <SidebarGroup className="pt-2">
                  <SidebarMenu>
                    <SettingsSection projectId={projectId} />
                  </SidebarMenu>
                </SidebarGroup>
              </SidebarContent>
            </>
          )}
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
