import { useMemo, useState } from "react";

import EditorPageWrapper from "../components/EditorPageWrapper";
import { useEditorContext } from "../contexts/useEditorContext";
import ModeSwitcher from "../View/components/ModeSwitcher";
import { ViewMode } from "../View/components/types";
import SemanticQueryPanel from "../components/SemanticQueryPanel";
import FieldsSelectionPanel from "./FieldsSelectionPanel";
import {
  TopicExplorerProvider,
  useTopicExplorerContext,
} from "./contexts/TopicExplorerContext";

const TopicExplorer = () => {
  const {
    topicData,
    viewsWithData,
    selectedDimensions,
    loadingTopicError,
    selectedMeasures,
    topicLoading,
    toggleDimension,
    toggleMeasure,
  } = useTopicExplorerContext();

  if (loadingTopicError) {
    return (
      <div className="flex flex-1 flex-col h-full items-center justify-center p-4">
        <div className="text-destructive text-center max-w-2xl">
          <div className="font-semibold mb-2">Error loading topic</div>
          <div className="text-sm">{loadingTopicError}</div>
        </div>
      </div>
    );
  }

  if (topicLoading || !topicData) {
    return (
      <div className="flex flex-1 flex-col h-full items-center justify-center">
        <div className="text-muted-foreground">Loading topic data...</div>
      </div>
    );
  }

  return (
    <div className="flex-1 flex flex-col min-h-0">
      <div className="flex-1 flex min-h-0">
        {/* Left Sidebar - Tree Structure */}
        <FieldsSelectionPanel
          topicData={topicData}
          viewsWithData={viewsWithData}
          isLoading={topicLoading}
          selectedDimensions={selectedDimensions}
          selectedMeasures={selectedMeasures}
          toggleDimension={toggleDimension}
          toggleMeasure={toggleMeasure}
        />

        {/* Right Side - Results and SQL */}
        <SemanticQueryPanel />
      </div>
    </div>
  );
};

type TopicPreviewProps = {
  pathb64: string;
  isReadOnly: boolean;
  gitEnabled: boolean;
};

const TopicPreview = (props: TopicPreviewProps) => {
  const { pathb64, isReadOnly } = props;
  const path = useMemo(() => atob(pathb64 || ""), [pathb64]);

  const [viewMode, setViewMode] = useState<ViewMode>(ViewMode.Explorer);

  return (
    <div className="flex flex-1 flex-col h-full">
      <div className="flex items-center justify-start p-1 border-b border-b-border gap-1">
        <ModeSwitcher viewMode={viewMode} onViewModeChange={setViewMode} />
        <div className="text-sm font-medium text-muted-foreground">{path}</div>
      </div>
      {viewMode === ViewMode.Explorer ? (
        <TopicExplorer />
      ) : (
        <EditorPageWrapper
          pathb64={pathb64}
          readOnly={isReadOnly}
          defaultDirection="horizontal"
          preview={<TopicExplorer />}
        />
      )}
    </div>
  );
};

const TopicEditor = () => {
  const { pathb64, isReadOnly, gitEnabled } = useEditorContext();

  return (
    <TopicExplorerProvider>
      <TopicPreview
        pathb64={pathb64}
        isReadOnly={isReadOnly}
        gitEnabled={gitEnabled}
      />
    </TopicExplorerProvider>
  );
};

export default TopicEditor;
