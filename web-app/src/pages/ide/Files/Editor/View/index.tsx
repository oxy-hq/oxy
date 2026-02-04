import { useEffect, useMemo, useState } from "react";
import { useFilesContext } from "../../FilesContext";
import { FilesSubViewMode } from "../../FilesSidebar/constants";
import EditorPageWrapper from "../components/EditorPageWrapper";
import SemanticQueryPanel from "../components/SemanticQueryPanel";
import { useEditorContext } from "../contexts/useEditorContext";
import ModeSwitcher from "./components/ModeSwitcher";
import { ViewMode } from "./components/types";
import { useViewExplorerContext, ViewExplorerProvider } from "./contexts/ViewExplorerContext";
import FieldsSelectionPanel from "./FieldsSelectionPanel";

type ViewPreviewProps = {
  pathb64: string;
  isReadOnly: boolean;
  gitEnabled: boolean;
};

const ViewPreview = (props: ViewPreviewProps) => {
  const { pathb64, isReadOnly } = props;
  const path = useMemo(() => atob(pathb64 || ""), [pathb64]);
  const { filesSubViewMode } = useFilesContext();

  // Default to Explorer for object mode, Editor for file mode
  const defaultViewMode =
    filesSubViewMode === FilesSubViewMode.OBJECTS ? ViewMode.Explorer : ViewMode.Editor;

  const [viewMode, setViewMode] = useState<ViewMode>(defaultViewMode);

  // Update view mode when sidebar mode changes
  useEffect(() => {
    setViewMode(defaultViewMode);
  }, [defaultViewMode]);

  return (
    <div className='flex h-full flex-1 flex-col'>
      <div className='flex items-center justify-start gap-1 border-b border-b-border p-1'>
        <ModeSwitcher viewMode={viewMode} onViewModeChange={setViewMode} />
        <div className='font-medium text-muted-foreground text-sm'>{path}</div>
      </div>
      <EditorPageWrapper
        pathb64={pathb64}
        readOnly={isReadOnly}
        defaultDirection='horizontal'
        preview={<ViewExplorer />}
        previewOnly={viewMode === ViewMode.Explorer}
      />
    </div>
  );
};

const ViewEditor = () => {
  const { pathb64, isReadOnly, gitEnabled } = useEditorContext();

  return (
    <ViewExplorerProvider>
      <ViewPreview pathb64={pathb64} isReadOnly={isReadOnly} gitEnabled={gitEnabled} />
    </ViewExplorerProvider>
  );
};

const ViewExplorer = () => {
  const { viewData, viewError, viewLoading } = useViewExplorerContext();

  if (viewError) {
    return (
      <div className='flex h-full flex-1 flex-col items-center justify-center p-4'>
        <div className='max-w-2xl text-center text-destructive'>
          <div className='mb-2 font-semibold'>Error loading view</div>
          <div className='text-sm'>{viewError?.message}</div>
        </div>
      </div>
    );
  }
  if (viewLoading || !viewData) {
    return (
      <div className='flex h-full flex-1 flex-col items-center justify-center'>
        <div className='text-muted-foreground'>Loading view data...</div>
      </div>
    );
  }

  return (
    <div className='flex min-h-0 flex-1 flex-col'>
      <div className='flex min-h-0 flex-1 gap-4'>
        {/* Left Sidebar - Tree Structure */}
        <FieldsSelectionPanel />

        {/* Right Side - Results and SQL */}
        <SemanticQueryPanel />
      </div>
    </div>
  );
};

export default ViewEditor;
