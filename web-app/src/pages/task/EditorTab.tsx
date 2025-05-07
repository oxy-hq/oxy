import { useState } from "react";
import FileEditor, { FileState } from "@/components/FileEditor";
import { useQueryClient } from "@tanstack/react-query";
import queryKeys from "@/hooks/api/queryKey";
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/shadcn/tabs";
import { TabsContent } from "@radix-ui/react-tabs";
import { Skeleton } from "@/components/ui/shadcn/skeleton";
import AppPreview from "@/components/AppPreview";
import EditorHeader from "../ide/Editor/components/EditorHeader";

// eslint-disable-next-line sonarjs/pseudo-random
const randomKey = () => Math.random().toString(36).substring(2, 15);

const EditorTab = ({ pathb64 }: { pathb64?: string }) => {
  const filePath = atob(pathb64 ?? "");
  const [fileState, setFileState] = useState<FileState>("saved");
  const [previewKey, setPreviewKey] = useState<string>(randomKey());
  const queryClient = useQueryClient();

  const onSaveApp = async () => {
    if (!pathb64) return;
    setPreviewKey(randomKey());
    await queryClient.invalidateQueries({
      queryKey: queryKeys.app.get(pathb64),
    });
    await queryClient.refetchQueries({
      queryKey: queryKeys.app.get(pathb64),
    });
  };

  return (
    <Tabs defaultValue="preview" className="flex flex-1 flex-col h-full">
      <TabsList className="flex gap-2 shrink-0 mt-2 ml-4">
        <TabsTrigger value="preview">Preview</TabsTrigger>
        <TabsTrigger value="editor" disabled={!filePath}>
          Code
        </TabsTrigger>
      </TabsList>
      <div className="flex-1 flex-col flex overflow-hidden">
        <TabsContent
          className="w-full flex flex-col items-center h-full gap-4"
          value="preview"
        >
          {!filePath && (
            <>
              <div>
                <div className="space-y-2">
                  <Skeleton className="h-4 w-[250px]" />
                  <Skeleton className="h-4 w-[250px]" />
                  <Skeleton className="h-4 w-[200px]" />
                </div>
              </div>
              <div>
                <Skeleton className="h-[200px] w-[250px]" />
              </div>
              <div>
                <Skeleton className="h-[200px] w-[250px]" />
              </div>
            </>
          )}
          <div className="flex-1 min-h-0 overflow-hidden w-full">
            <AppPreview key={previewKey} appPath64={pathb64 ?? ""} />
          </div>
        </TabsContent>
        <TabsContent
          value="editor"
          className="flex-1 flex flex-col bg-editor-background"
        >
          <EditorHeader filePath={filePath} fileState={fileState} />
          <FileEditor
            fileState={fileState}
            pathb64={pathb64 ?? ""}
            onFileStateChange={setFileState}
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
