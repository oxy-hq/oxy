import { useEffect, useMemo, useState } from "react";
import { decodeBase64 } from "@/libs/encoding";
import { useFilesContext } from "../../FilesContext";
import { FilesSubViewMode } from "../../FilesSidebar/constants";
import EditorPageWrapper from "../components/EditorPageWrapper";
import SemanticQueryPanel from "../components/SemanticQueryPanel";
import { useEditorContext } from "../contexts/useEditorContext";
import ModeSwitcher from "../View/components/ModeSwitcher";
import { ViewMode } from "../View/components/types";
import { TopicExplorerProvider, useTopicExplorerContext } from "./contexts/TopicExplorerContext";
import FieldsSelectionPanel from "./FieldsSelectionPanel";

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
    timeDimensions,
    onAddTimeDimension,
    onUpdateTimeDimension,
    onRemoveTimeDimension
  } = useTopicExplorerContext();

  if (loadingTopicError) {
    return (
      <div className='flex h-full flex-1 flex-col items-center justify-center p-4'>
        <div className='max-w-2xl text-center text-destructive'>
          <div className='mb-2 font-semibold'>Error loading topic</div>
          <div className='text-sm'>{loadingTopicError}</div>
        </div>
      </div>
    );
  }

  if (topicLoading || !topicData) {
    return (
      <div className='flex h-full flex-1 flex-col items-center justify-center'>
        <div className='text-muted-foreground'>Loading topic data...</div>
      </div>
    );
  }

  return (
    <div className='flex min-h-0 flex-1 flex-col'>
      <div className='flex min-h-0 flex-1'>
        {/* Left Sidebar - Tree Structure */}
        <FieldsSelectionPanel
          topicData={topicData}
          viewsWithData={viewsWithData}
          isLoading={topicLoading}
          selectedDimensions={selectedDimensions}
          selectedMeasures={selectedMeasures}
          toggleDimension={toggleDimension}
          toggleMeasure={toggleMeasure}
          timeDimensions={timeDimensions}
          onAddTimeDimension={onAddTimeDimension}
          onUpdateTimeDimension={onUpdateTimeDimension}
          onRemoveTimeDimension={onRemoveTimeDimension}
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
  const path = useMemo(() => decodeBase64(pathb64 || ""), [pathb64]);
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
        preview={<TopicExplorer />}
        previewOnly={viewMode === ViewMode.Explorer}
      />
    </div>
  );
};

const TopicEditor = () => {
  const { pathb64, isReadOnly, gitEnabled } = useEditorContext();

  return (
    <TopicExplorerProvider>
      <TopicPreview pathb64={pathb64} isReadOnly={isReadOnly} gitEnabled={gitEnabled} />
    </TopicExplorerProvider>
  );
};

export default TopicEditor;
