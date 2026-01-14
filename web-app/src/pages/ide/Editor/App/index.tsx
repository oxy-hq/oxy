import { useMemo, useState } from "react";
import { useEditorContext } from "../contexts/useEditorContext";
import { usePreviewRefresh } from "../usePreviewRefresh";
import { useEditorQueryInvalidation } from "../useEditorQueryInvalidation";
import YAML from "yaml";
import { AppViewMode } from "./types";
import { ModeSwitcher } from "./components/ModeSwitcher";
import { VisualizationMode } from "./components/VisualizationMode";
import { EditorFormMode } from "./components/EditorFormMode";

const AppEditor = () => {
  const { pathb64, isReadOnly, gitEnabled } = useEditorContext();
  const { previewKey, refreshPreview } = usePreviewRefresh();
  const { invalidateAppQueries } = useEditorQueryInvalidation();
  const [viewMode, setViewMode] = useState<AppViewMode>(
    AppViewMode.Visualization,
  );
  const [validationError, setValidationError] = useState<string | null>(null);

  const appPath = useMemo(() => atob(pathb64 ?? ""), [pathb64]);

  const validateContent = (value: string) => {
    try {
      YAML.parse(value);
      setValidationError(null);
    } catch (error) {
      const errorMessage =
        error instanceof Error ? error.message : "Invalid YAML format";
      setValidationError(errorMessage);
    }
  };

  const handleSaved = () => {
    refreshPreview();
    invalidateAppQueries();
  };

  const modeSwitcher = (
    <ModeSwitcher viewMode={viewMode} setViewMode={setViewMode} />
  );

  // Render full-screen visualization mode
  if (viewMode === AppViewMode.Visualization) {
    return (
      <VisualizationMode
        modeSwitcher={modeSwitcher}
        appPath={appPath}
        previewKey={previewKey}
        pathb64={pathb64}
      />
    );
  }

  // Render editor or form mode with EditorPageWrapper
  return (
    <EditorFormMode
      modeSwitcher={modeSwitcher}
      appPath={appPath}
      validationError={validationError}
      pathb64={pathb64}
      handleSaved={handleSaved}
      isReadOnly={isReadOnly}
      gitEnabled={gitEnabled}
      viewMode={viewMode}
      validateContent={validateContent}
      previewKey={previewKey}
    />
  );
};
export default AppEditor;
