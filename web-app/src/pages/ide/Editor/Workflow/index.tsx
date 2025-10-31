import EditorPageWrapper from "../components/EditorPageWrapper";
import { WorkflowPreview } from "@/components/workflow/WorkflowPreview";
import { useEditorContext } from "../contexts/useEditorContext";
import { usePreviewRefresh } from "../usePreviewRefresh";
import { useSearchParams } from "react-router-dom";

const WorkflowEditor = () => {
  const { pathb64, isReadOnly, gitEnabled } = useEditorContext();
  const { previewKey, refreshPreview } = usePreviewRefresh();
  const [searchParams] = useSearchParams();
  const runId = searchParams.get("run") || undefined;

  return (
    <EditorPageWrapper
      pathb64={pathb64}
      readOnly={isReadOnly}
      onSaved={refreshPreview}
      preview={
        <div className="flex-1 overflow-hidden">
          <WorkflowPreview
            key={previewKey + runId}
            pathb64={pathb64}
            runId={runId}
            direction="vertical"
          />
        </div>
      }
      git={gitEnabled}
    />
  );
};
export default WorkflowEditor;
