import { useState } from "react";
import AppPreview from "@/components/AppPreview";
import { useFilesContext } from "../../FilesContext";
import { FilesSubViewMode } from "../../FilesSidebar/constants";
import EditorPageWrapper from "../components/EditorPageWrapper";
import { useEditorContext } from "../contexts/useEditorContext";
import { useEditorQueryInvalidation } from "../useEditorQueryInvalidation";
import { usePreviewRefresh } from "../usePreviewRefresh";
import { AppFormWrapper } from "./components/AppFormWrapper";
import { ModeSwitcher } from "./components/ModeSwitcher";
import { AppViewMode } from "./types";

const AppEditor = () => {
  const { pathb64, isReadOnly, gitEnabled } = useEditorContext();
  const { previewKey, refreshPreview } = usePreviewRefresh();
  const { invalidateAppQueries } = useEditorQueryInvalidation();
  const { filesSubViewMode } = useFilesContext();

  const defaultViewMode =
    filesSubViewMode === FilesSubViewMode.OBJECTS ? AppViewMode.Form : AppViewMode.Visualization;

  const [viewMode, setViewMode] = useState<AppViewMode>(defaultViewMode);

  const handleSaved = () => {
    refreshPreview();
    invalidateAppQueries();
  };

  // Render editor or form mode with EditorPageWrapper
  return (
    <EditorPageWrapper
      headerPrefixAction={<ModeSwitcher viewMode={viewMode} setViewMode={setViewMode} />}
      pathb64={pathb64}
      onSaved={handleSaved}
      readOnly={isReadOnly}
      git={gitEnabled}
      customEditor={viewMode === AppViewMode.Form ? <AppFormWrapper /> : undefined}
      previewOnly={viewMode === AppViewMode.Visualization}
      preview={
        <div className='flex-1 overflow-hidden'>
          <AppPreview key={previewKey} appPath64={pathb64} />
        </div>
      }
    />
  );
};
export default AppEditor;
