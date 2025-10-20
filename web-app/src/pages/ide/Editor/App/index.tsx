import EditorPageWrapper from "../components/EditorPageWrapper";
import AppPreview from "@/components/AppPreview";
import { useEditorContext } from "../contexts/useEditorContext";
import { usePreviewRefresh } from "../usePreviewRefresh";
import { useEditorQueryInvalidation } from "../useEditorQueryInvalidation";

const AppEditor = () => {
  const { pathb64, isReadOnly, gitEnabled } = useEditorContext();
  const { previewKey, refreshPreview } = usePreviewRefresh();
  const { invalidateAppQueries } = useEditorQueryInvalidation();

  const handleSaved = () => {
    refreshPreview();
    invalidateAppQueries();
  };

  return (
    <EditorPageWrapper
      pathb64={pathb64}
      onSaved={handleSaved}
      readOnly={isReadOnly}
      git={gitEnabled}
      preview={
        <div className="flex-1 overflow-hidden">
          <AppPreview key={previewKey} appPath64={pathb64} />
        </div>
      }
    />
  );
};
export default AppEditor;
