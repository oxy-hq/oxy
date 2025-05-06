import { useState } from "react";
import FileEditor, { FileState } from "@/components/FileEditor";
import WorkflowPreview from "@/pages/workflow/WorkflowPreview";
import { useQueryClient } from "@tanstack/react-query";
import queryKeys from "@/hooks/api/queryKey";
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/shadcn/tabs";
import { TabsContent } from "@radix-ui/react-tabs";
import { Skeleton } from "@/components/ui/shadcn/skeleton";
import AgentPreview from "./Agent/Preview";
import AppPreview from "./App/AppPreview";
import EditorHeader from "./components/EditorHeader";

// eslint-disable-next-line sonarjs/pseudo-random
const randomKey = () => Math.random().toString(36).substring(2, 15);

const EditorTab = ({ pathb64 }: { pathb64?: string }) => {
  const filePath = atob(pathb64 ?? "");
  const isWorkflow = filePath.endsWith(".workflow.yml");
  const isAgent = filePath.endsWith(".agent.yml");
  const isApp = filePath.endsWith(".app.yml");
  const [fileState, setFileState] = useState<FileState>("saved");
  const [previewKey, setPreviewKey] = useState<string>(randomKey());
  const queryClient = useQueryClient();

  const onSaveApp = async () => {
    if (!pathb64) return;
    await queryClient.invalidateQueries({
      queryKey: queryKeys.app.get(pathb64),
    });
    await queryClient.refetchQueries({
      queryKey: queryKeys.app.get(pathb64),
    });
  };

  return (
    <Tabs defaultValue="preview" className="flex flex-1 flex-col h-full">
      <TabsList className="flex gap-2 shrink-0 m-1">
        <TabsTrigger value="preview">Preview</TabsTrigger>
        <TabsTrigger value="editor" disabled={!filePath}>
          Code
        </TabsTrigger>
      </TabsList>
      <div className="flex-1 overflow-hidden">
        <TabsContent
          className="w-full flex flex-col items-center h-full"
          value="preview"
        >
          {!filePath && (
            <div className="flex flex-col gap-4 w-full h-full items-center justify-center">
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
            </div>
          )}
          {isWorkflow && (
            <div className="flex-1">
              <WorkflowPreview key={previewKey} pathb64={pathb64 ?? ""} />
            </div>
          )}
          {isAgent && (
            <div className="flex-1">
              <AgentPreview key={previewKey} agentPathb64={pathb64 ?? ""} />
            </div>
          )}
          {isApp && (
            <div className="flex-1 min-h-0 overflow-hidden w-full">
              <AppPreview appPath64={pathb64 ?? ""} />
            </div>
          )}
        </TabsContent>
        <TabsContent value="editor" className="w-full h-full flex-1">
          <div className="flex h-full md:flex-row flex-col">
            <div className="flex-1 md:w-[50%] w-full md:h-full h-[50%] flex flex-col bg-[#1e1e1e]">
              <EditorHeader filePath={filePath} fileState={fileState} />
              <FileEditor
                fileState={fileState}
                pathb64={pathb64 ?? ""}
                onFileStateChange={setFileState}
                onSaved={() => {
                  if (isApp) {
                    onSaveApp();
                  } else {
                    setPreviewKey(randomKey());
                  }
                }}
              />
            </div>
          </div>
        </TabsContent>
      </div>
    </Tabs>
  );
};

export default EditorTab;
