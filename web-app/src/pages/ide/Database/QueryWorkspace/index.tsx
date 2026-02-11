import { useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { toast } from "sonner";
import {
  ResizableHandle,
  ResizablePanel,
  ResizablePanelGroup
} from "@/components/ui/shadcn/resizable";
import queryKeys from "@/hooks/api/queryKey";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { encodeBase64 } from "@/libs/encoding";
import { FileService } from "@/services/api";
import useDatabaseClient from "@/stores/useDatabaseClient";
import QueryEditor from "./components/QueryEditor";
import QueryResults from "./components/QueryResults";
import SaveQueryDialog from "./components/SaveQueryDialog";

export default function QueryWorkspacePage() {
  const { tabs, activeTabId, updateTab } = useDatabaseClient();
  const { project, branchName } = useCurrentProjectBranch();
  const queryClient = useQueryClient();
  const [isSaveDialogOpen, setIsSaveDialogOpen] = useState(false);

  const activeTab = tabs.find((t) => t.id === activeTabId);

  const handleSaveQuery = async () => {
    if (!activeTab) return;

    // If the tab already has a saved path, save directly without showing dialog
    if (activeTab.savedPath) {
      try {
        const pathb64 = encodeBase64(activeTab.savedPath);
        await FileService.saveFile(project.id, pathb64, activeTab.content, branchName);

        updateTab(activeTab.id, { isDirty: false });

        // Invalidate file tree query to refresh the sidebar
        queryClient.removeQueries({
          queryKey: queryKeys.file.tree(project.id, branchName)
        });

        toast.success(`Saved to ${activeTab.savedPath}`);
      } catch (error) {
        toast.error(error instanceof Error ? error.message : "Failed to save");
      }
    } else {
      // Show dialog to get filename for new file
      setIsSaveDialogOpen(true);
    }
  };

  return (
    <div className='flex h-full flex-col'>
      <div className='flex-1 overflow-hidden'>
        <ResizablePanelGroup direction='vertical'>
          <ResizablePanel defaultSize={60} minSize={30}>
            <QueryEditor onSave={handleSaveQuery} />
          </ResizablePanel>

          <ResizableHandle withHandle />

          <ResizablePanel defaultSize={40} minSize={20}>
            <QueryResults />
          </ResizablePanel>
        </ResizablePanelGroup>
      </div>

      <SaveQueryDialog open={isSaveDialogOpen} onOpenChange={setIsSaveDialogOpen} tab={activeTab} />
    </div>
  );
}
