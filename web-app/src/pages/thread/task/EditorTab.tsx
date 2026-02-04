import { TabsContent } from "@radix-ui/react-tabs";
import { useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import AppPreview from "@/components/AppPreview";
import { Skeleton } from "@/components/ui/shadcn/skeleton";
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/shadcn/tabs";
import queryKeys from "@/hooks/api/queryKey";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import EditorPageWrapper from "@/pages/ide/Files/Editor/components/EditorPageWrapper";

// eslint-disable-next-line sonarjs/pseudo-random
const randomKey = () => Math.random().toString(36).substring(2, 15);

const EditorTab = ({ pathb64 }: { pathb64?: string }) => {
  const filePath = atob(pathb64 ?? "");
  const [previewKey, setPreviewKey] = useState<string>(randomKey());
  const queryClient = useQueryClient();
  const { project, branchName } = useCurrentProjectBranch();
  const projectId = project.id;

  const onSaveApp = async () => {
    if (!pathb64) return;
    setPreviewKey(randomKey());
    await queryClient.invalidateQueries({
      queryKey: queryKeys.app.getAppData(projectId, branchName, pathb64)
    });
    await queryClient.refetchQueries({
      queryKey: queryKeys.app.getAppData(projectId, branchName, pathb64)
    });
  };

  return (
    <Tabs defaultValue='preview' className='flex h-full flex-1 flex-col'>
      <TabsList className='mt-2 ml-4 flex shrink-0 gap-2'>
        <TabsTrigger value='preview'>Preview</TabsTrigger>
        <TabsTrigger value='editor' disabled={!filePath}>
          Code
        </TabsTrigger>
      </TabsList>
      <div className='flex flex-1 flex-col overflow-hidden'>
        <TabsContent className='flex h-full w-full flex-col items-center gap-4' value='preview'>
          {!filePath ? (
            <div className='flex h-full w-full flex-col items-center justify-center'>
              <div className='flex flex-col gap-4'>
                <div className='space-y-2'>
                  <Skeleton className='h-4 w-[250px]' />
                  <Skeleton className='h-4 w-[250px]' />
                  <Skeleton className='h-4 w-[200px]' />
                </div>
                <Skeleton className='h-[200px] w-[250px]' />
                <Skeleton className='h-[200px] w-[250px]' />
              </div>
            </div>
          ) : (
            <div className='min-h-0 w-full flex-1 overflow-hidden'>
              <AppPreview key={previewKey} appPath64={pathb64 ?? ""} />
            </div>
          )}
        </TabsContent>
        <TabsContent value='editor' className='flex flex-1 flex-col bg-editor-background'>
          <EditorPageWrapper
            pathb64={pathb64 ?? ""}
            pageContentClassName='md:flex-row flex-col'
            editorClassName='w-full h-full'
            readOnly={true}
            onSaved={() => {
              onSaveApp();
            }}
          />
        </TabsContent>
      </div>
    </Tabs>
  );
};

export default EditorTab;
