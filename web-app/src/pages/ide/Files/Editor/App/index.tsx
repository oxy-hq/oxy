import { useMemo, useState } from "react";
import YAML from "yaml";
import { decodeBase64 } from "@/libs/encoding";
import { useFilesContext } from "../../FilesContext";
import { FilesSubViewMode } from "../../FilesSidebar/constants";
import { useEditorContext } from "../contexts/useEditorContext";
import { useEditorQueryInvalidation } from "../useEditorQueryInvalidation";
import { usePreviewRefresh } from "../usePreviewRefresh";
import { EditorFormMode } from "./components/EditorFormMode";
import { ModeSwitcher } from "./components/ModeSwitcher";
import { AppViewMode } from "./types";

const AppEditor = () => {
  const { pathb64, isReadOnly, gitEnabled } = useEditorContext();
  const { previewKey, refreshPreview } = usePreviewRefresh();
  const { invalidateAppQueries } = useEditorQueryInvalidation();
  const { filesSubViewMode } = useFilesContext();

  // Default to Form for object mode (GUI editor), Visualization for file mode
  const defaultViewMode =
    filesSubViewMode === FilesSubViewMode.OBJECTS ? AppViewMode.Form : AppViewMode.Visualization;

  const [viewMode, setViewMode] = useState<AppViewMode>(defaultViewMode);

  const [validationError, setValidationError] = useState<string | null>(null);

  const appPath = useMemo(() => decodeBase64(pathb64 ?? ""), [pathb64]);

  const validateContent = (value: string) => {
    try {
      YAML.parse(value);
      setValidationError(null);
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : "Invalid YAML format";
      setValidationError(errorMessage);
    }
  };

  const handleSaved = () => {
    refreshPreview();
    invalidateAppQueries();
  };

  const modeSwitcher = <ModeSwitcher viewMode={viewMode} setViewMode={setViewMode} />;

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
