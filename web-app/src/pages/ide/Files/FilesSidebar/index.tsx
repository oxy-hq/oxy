import { FilePlus, Folder, FolderPlus, Layers2, Loader2, RotateCw } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { useParams } from "react-router-dom";
import { Button } from "@/components/ui/shadcn/button";
import { SidebarContent, SidebarGroup, SidebarMenu } from "@/components/ui/shadcn/sidebar";
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/shadcn/tabs";
import useFileTree from "@/hooks/api/files/useFileTree";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { decodeBase64 } from "@/libs/encoding";
import { SidebarHeader } from "@/pages/ide/components/SidebarHeader";
import { useFilesContext } from "../FilesContext";
import { FilesSubViewMode, IGNORE_FILES_REGEX, NAME_COLLATOR } from "./constants";
import FileTreeNode from "./FileTreeNode";
import GroupedObjectsView from "./GroupedObjectsView";
import NewNode, { type CreationType } from "./NewNode";
import NewObjectButton from "./NewObjectButton";
import { getAllObjectFiles } from "./utils";

export const SIDEBAR_REVEAL_FILE = "sidebar:reveal-file";

const FilesSidebar: React.FC<{
  setSidebarOpen: (open: boolean) => void;
}> = ({ setSidebarOpen }) => {
  const { isReadOnly, project } = useCurrentProjectBranch();
  const projectId = project.id;
  const { filesSubViewMode, setFilesSubViewMode } = useFilesContext();
  const { data, refetch, isPending } = useFileTree();
  const { pathb64 } = useParams();

  const activePath = useMemo(() => {
    if (!pathb64) return undefined;
    try {
      return decodeBase64(pathb64);
    } catch {
      return undefined;
    }
  }, [pathb64]);

  const [isCreating, setIsCreating] = useState(false);
  const [creationType, setCreationType] = useState<CreationType>("file");

  const handleCreateFile = () => {
    setCreationType("file");
    setIsCreating(true);
  };

  const handleCreateFolder = () => {
    setCreationType("folder");
    setIsCreating(true);
  };

  const fileTree = useMemo(() => {
    const filtered = data?.filter((f) => !IGNORE_FILES_REGEX.some((r) => f.name.match(r)));

    if (!filtered) return undefined;

    return filtered.sort((a, b) => {
      if (a.is_dir && !b.is_dir) return -1;
      if (!a.is_dir && b.is_dir) return 1;
      return NAME_COLLATOR.compare(a.name, b.name);
    });
  }, [data]);

  useEffect(() => {
    if (activePath) {
      try {
        window.dispatchEvent(
          new CustomEvent(SIDEBAR_REVEAL_FILE, {
            detail: { path: activePath }
          })
        );
      } catch {
        // ignore
      }
    }
  }, [activePath]);

  return (
    <div className='flex h-full flex-col overflow-hidden bg-sidebar-background'>
      <SidebarHeader
        title={filesSubViewMode === FilesSubViewMode.OBJECTS ? "Objects" : "Files"}
        onCollapse={() => setSidebarOpen(false)}
        actions={
          <>
            {!isReadOnly && (
              <>
                {filesSubViewMode === FilesSubViewMode.OBJECTS && <NewObjectButton />}
                {filesSubViewMode === FilesSubViewMode.FILES && (
                  <>
                    <Button
                      type='button'
                      variant='ghost'
                      size='sm'
                      onClick={handleCreateFile}
                      tooltip='New File'
                    >
                      <FilePlus className='h-4 w-4' />
                    </Button>
                    <Button
                      type='button'
                      variant='ghost'
                      size='sm'
                      onClick={handleCreateFolder}
                      tooltip='New Folder'
                    >
                      <FolderPlus className='h-4 w-4' />
                    </Button>
                  </>
                )}
              </>
            )}
            {filesSubViewMode === FilesSubViewMode.FILES && (
              <Button
                type='button'
                variant='ghost'
                size='sm'
                onClick={() => refetch()}
                tooltip='Refresh'
              >
                <RotateCw className='h-4 w-4' />
              </Button>
            )}
          </>
        }
      />

      <div className='border-sidebar-border border-b px-2 py-2'>
        <Tabs
          value={filesSubViewMode}
          onValueChange={(v) => setFilesSubViewMode(v as FilesSubViewMode)}
          className='w-full'
        >
          <TabsList className='grid h-7 w-full grid-cols-2'>
            <TabsTrigger value='objects' className='h-6 gap-1 text-xs'>
              <Layers2 className='h-3 w-3' />
              Objects
            </TabsTrigger>
            <TabsTrigger value='files' className='h-6 gap-1 text-xs'>
              <Folder className='h-3 w-3' />
              Files
            </TabsTrigger>
          </TabsList>
        </Tabs>
      </div>

      <SidebarContent className='customScrollbar h-full flex-1 overflow-y-auto'>
        <SidebarGroup className='pt-2'>
          {isPending && (
            <div className='flex items-center justify-center p-4'>
              <Loader2 className='h-4 w-4 animate-spin' />
            </div>
          )}

          {filesSubViewMode === FilesSubViewMode.OBJECTS && data && !isPending && (
            <GroupedObjectsView
              files={getAllObjectFiles(
                data.filter((f) => !IGNORE_FILES_REGEX.some((r) => f.name.match(r)))
              )}
              projectId={projectId}
              activePath={activePath}
            />
          )}

          {filesSubViewMode === FilesSubViewMode.FILES && fileTree && !isPending && (
            <SidebarMenu className='pb-20'>
              {isCreating && (
                <NewNode
                  currentPath=''
                  creationType={creationType}
                  onCreated={() => {
                    setIsCreating(false);
                    refetch();
                  }}
                  onCancel={() => setIsCreating(false)}
                />
              )}
              {fileTree.map((f) => (
                <FileTreeNode key={f.path} fileTree={f} activePath={activePath} />
              ))}
            </SidebarMenu>
          )}
        </SidebarGroup>
      </SidebarContent>
    </div>
  );
};

export default FilesSidebar;
