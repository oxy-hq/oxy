import EditorPreview from "../components/SemanticExplorer/EditorPreview";
import SemanticQueryPanel from "../components/SemanticQueryPanel";
import { useEditorContext } from "../contexts/useEditorContext";
import { useViewExplorerContext, ViewExplorerProvider } from "./contexts/ViewExplorerContext";
import FieldsSelectionPanel from "./FieldsSelectionPanel";

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
      <div className='flex min-h-0 flex-1'>
        <FieldsSelectionPanel />
        <SemanticQueryPanel />
      </div>
    </div>
  );
};

const ViewEditor = () => {
  const { pathb64, isReadOnly } = useEditorContext();

  return (
    <ViewExplorerProvider>
      <EditorPreview pathb64={pathb64} isReadOnly={isReadOnly} explorer={<ViewExplorer />} />
    </ViewExplorerProvider>
  );
};

export default ViewEditor;
