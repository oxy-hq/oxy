import ErrorAlert from "@/components/ui/ErrorAlert";
import EditorPreview from "../components/SemanticExplorer/EditorPreview";
import SemanticQueryPanel from "../components/SemanticQueryPanel";
import { useEditorContext } from "../contexts/useEditorContext";
import { TopicExplorerProvider, useTopicExplorerContext } from "./contexts/TopicExplorerContext";
import FieldsSelectionPanel from "./FieldsSelectionPanel";

const TopicExplorer = () => {
  const { loadingTopicError, topicLoading, topicData } = useTopicExplorerContext();

  if (loadingTopicError) {
    return (
      <div className='flex h-full flex-1 flex-col items-center justify-center p-4'>
        <ErrorAlert title='Error loading topic' message={loadingTopicError} className='max-w-2xl' />
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
        <FieldsSelectionPanel />
        <SemanticQueryPanel />
      </div>
    </div>
  );
};

const TopicEditor = () => {
  const { pathb64 } = useEditorContext();

  return (
    <TopicExplorerProvider>
      <EditorPreview pathb64={pathb64} explorer={<TopicExplorer />} />
    </TopicExplorerProvider>
  );
};

export default TopicEditor;
