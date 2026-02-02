import { useMemo, useState } from "react";
import { debounce } from "lodash";
import EditorPageWrapper from "../components/EditorPageWrapper";
import { useEditorContext } from "../contexts/useEditorContext";
import { useEditorQueryInvalidation } from "../useEditorQueryInvalidation";
import { usePreviewRefresh } from "../usePreviewRefresh";
import { AgentForm, AgentFormData } from "@/components/agent/AgentForm";
import { useFileEditorContext } from "@/components/FileEditor/useFileEditorContext";
import YAML from "yaml";
import PreviewSection from "./PreviewSection";
import ViewModeToggle from "./components/ViewModeToggle";
import { AgentViewMode } from "./types";
import { useFilesContext } from "../../FilesContext";
import { FilesSubViewMode } from "../../FilesSidebar/constants";

const AgentEditor = () => {
  const { pathb64, isReadOnly, gitEnabled } = useEditorContext();
  const { previewKey, refreshPreview } = usePreviewRefresh();
  const { filesSubViewMode } = useFilesContext();

  // Default to Form for object mode (GUI editor), Editor for file mode
  const defaultViewMode =
    filesSubViewMode === FilesSubViewMode.OBJECTS
      ? AgentViewMode.Form
      : AgentViewMode.Editor;

  const [viewMode, setViewMode] = useState<AgentViewMode>(defaultViewMode);

  const [validationError, setValidationError] = useState<string | null>(null);
  const { invalidateAgentQueries } = useEditorQueryInvalidation();

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
    invalidateAgentQueries();
  };

  return (
    <EditorPageWrapper
      headerActions={
        <ViewModeToggle
          viewMode={viewMode}
          onViewModeChange={setViewMode}
          validationError={validationError}
        />
      }
      pathb64={pathb64}
      onSaved={handleSaved}
      readOnly={isReadOnly}
      git={gitEnabled}
      customEditor={
        viewMode === AgentViewMode.Form ? <AgentFormWrapper /> : undefined
      }
      onChanged={(value) => {
        if (viewMode === AgentViewMode.Editor) {
          validateContent(value);
        }
      }}
      preview={<PreviewSection pathb64={pathb64} previewKey={previewKey} />}
    />
  );
};
export default AgentEditor;

const AgentFormWrapper = () => {
  const { state, actions } = useFileEditorContext();

  const content = state.content;

  const data = useMemo(() => {
    try {
      if (!content) return undefined;
      const parsed = YAML.parse(content) as Partial<AgentFormData>;
      return parsed;
    } catch (error) {
      console.error("Failed to parse YAML content to form data:", error);
      return undefined;
    }
  }, [content]);

  const onChange = useMemo(
    () =>
      debounce((formData: AgentFormData) => {
        try {
          const yamlContent = YAML.stringify(formData, {
            indent: 2,
            lineWidth: 0,
          });
          actions.setContent(yamlContent);
        } catch (error) {
          console.error("Failed to serialize form data to YAML:", error);
        }
      }, 500),
    [actions],
  );

  if (!data) return null;

  return <AgentForm data={data} onChange={onChange} />;
};
