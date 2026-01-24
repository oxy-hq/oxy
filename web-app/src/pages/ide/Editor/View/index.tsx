import { useMemo, useState, useEffect } from "react";
import EditorPageWrapper from "../components/EditorPageWrapper";
import { useEditorContext } from "../contexts/useEditorContext";

import SemanticQueryPanel from "../components/SemanticQueryPanel";
import FieldsSelectionPanel from "./FieldsSelectionPanel";
import ModeSwitcher from "./components/ModeSwitcher";
import { ViewMode } from "./components/types";
import {
  ViewExplorerProvider,
  useViewExplorerContext,
} from "./contexts/ViewExplorerContext";
import { FilesSubViewMode, useIDE } from "../..";

type ViewPreviewProps = {
  pathb64: string;
  isReadOnly: boolean;
  gitEnabled: boolean;
};

const ViewPreview = (props: ViewPreviewProps) => {
  const { pathb64, isReadOnly } = props;
  const path = useMemo(() => atob(pathb64 || ""), [pathb64]);
  const { filesSubViewMode } = useIDE();

  // Default to Explorer for object mode, Editor for file mode
  const defaultViewMode =
    filesSubViewMode === FilesSubViewMode.OBJECTS
      ? ViewMode.Explorer
      : ViewMode.Editor;

  const [viewMode, setViewMode] = useState<ViewMode>(defaultViewMode);

  // Update view mode when sidebar mode changes
  useEffect(() => {
    setViewMode(defaultViewMode);
  }, [defaultViewMode]);

  return (
    <div className="flex flex-1 flex-col h-full">
      <div className="flex items-center justify-start p-1 border-b border-b-border gap-1">
        <ModeSwitcher viewMode={viewMode} onViewModeChange={setViewMode} />
        <div className="text-sm font-medium text-muted-foreground">{path}</div>
      </div>
      <EditorPageWrapper
        pathb64={pathb64}
        readOnly={isReadOnly}
        defaultDirection="horizontal"
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
      <ViewPreview
        pathb64={pathb64}
        isReadOnly={isReadOnly}
        gitEnabled={gitEnabled}
      />
    </ViewExplorerProvider>
  );
};

const ViewExplorer = () => {
  const { viewData, viewError, viewLoading } = useViewExplorerContext();

  if (viewError) {
    return (
      <div className="flex flex-1 flex-col h-full items-center justify-center p-4">
        <div className="text-destructive text-center max-w-2xl">
          <div className="font-semibold mb-2">Error loading view</div>
          <div className="text-sm">{viewError?.message}</div>
        </div>
      </div>
    );
  }
  if (viewLoading || !viewData) {
    return (
      <div className="flex flex-1 flex-col h-full items-center justify-center">
        <div className="text-muted-foreground">Loading view data...</div>
      </div>
    );
  }

  return (
    <div className="flex-1 flex flex-col min-h-0">
      <div className="flex-1 flex gap-4 min-h-0">
        {/* Left Sidebar - Tree Structure */}
        <FieldsSelectionPanel />

        {/* Right Side - Results and SQL */}
        <SemanticQueryPanel />
      </div>
    </div>
  );
};

export default ViewEditor;
