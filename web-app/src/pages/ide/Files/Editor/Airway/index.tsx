/**
 * IDE editor for `.airway.yml` pipeline files.
 *
 * Mirrors `WorkflowEditor`'s split layout but minimal: a `.airway.yml`
 * is plain YAML (no visual form / diagram), so the left pane is the
 * default Monaco editor and the right pane embeds the live run page.
 * The Run/Cancel control lives inside the embedded run page.
 */

import { AirwayPipelinePage } from "@/pages/airway";

import EditorPageWrapper from "../components/EditorPageWrapper";
import { useEditorContext } from "../contexts/useEditorContext";
import { usePreviewRefresh } from "../usePreviewRefresh";

const AirwayEditor = () => {
  const { pathb64, gitEnabled } = useEditorContext();
  const { refreshPreview, previewKey } = usePreviewRefresh();

  return (
    <EditorPageWrapper
      pathb64={pathb64}
      onSaved={refreshPreview}
      git={gitEnabled}
      preview={<AirwayPipelinePage key={previewKey} pathb64={pathb64} hideHeader />}
    />
  );
};

export default AirwayEditor;
