import EditorPageWrapper from "../components/EditorPageWrapper";
import { WorkflowPreview } from "@/components/workflow/WorkflowPreview";
import { useEditorContext } from "../contexts/useEditorContext";
import { usePreviewRefresh } from "../usePreviewRefresh";

const WorkflowEditor = () => {
  const { pathb64, isReadOnly, gitEnabled } = useEditorContext();
  const { previewKey, refreshPreview } = usePreviewRefresh();

  return (
    <EditorPageWrapper
      pathb64={pathb64}
      readOnly={isReadOnly}
      onSaved={refreshPreview}
      preview={
        <div className="flex-1 overflow-hidden">
          <WorkflowPreview key={previewKey} pathb64={pathb64} />
        </div>
      }
      git={gitEnabled}
    />
  );
};
export default WorkflowEditor;
