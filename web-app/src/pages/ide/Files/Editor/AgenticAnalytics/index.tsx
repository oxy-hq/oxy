import { debounce } from "lodash";
import { useMemo, useState } from "react";
import YAML from "yaml";
import {
  AgenticAnalyticsForm,
  type AgenticYamlData,
  yamlToForm
} from "@/components/agentic/AgenticAnalyticsForm";
import { useFileEditorContext } from "@/components/FileEditor/useFileEditorContext";
import { useFilesContext } from "../../FilesContext";
import { FilesSubViewMode } from "../../FilesSidebar/constants";
import ViewModeToggle from "../Agent/components/ViewModeToggle";
import type { AgentViewMode } from "../Agent/types";
import EditorPageWrapper from "../components/EditorPageWrapper";
import { useEditorContext } from "../contexts/useEditorContext";
import { usePreviewRefresh } from "../usePreviewRefresh";
import PreviewSection from "./PreviewSection";
import { AgenticViewMode } from "./types";

// AgenticViewMode and AgentViewMode share identical string values — cast is safe.
const toAgentViewMode = (m: AgenticViewMode): AgentViewMode => m as unknown as AgentViewMode;
const toAgenticViewMode = (m: AgentViewMode): AgenticViewMode => m as unknown as AgenticViewMode;

const AgenticAnalyticsEditor = () => {
  const { pathb64, gitEnabled } = useEditorContext();
  const { previewKey, refreshPreview } = usePreviewRefresh();
  const { filesSubViewMode } = useFilesContext();

  const defaultViewMode =
    filesSubViewMode === FilesSubViewMode.OBJECTS ? AgenticViewMode.Form : AgenticViewMode.Editor;

  const [viewMode, setViewMode] = useState<AgenticViewMode>(defaultViewMode);
  const [validationError, setValidationError] = useState<string | null>(null);

  const validateContent = (value: string) => {
    try {
      YAML.parse(value);
      setValidationError(null);
    } catch (error) {
      setValidationError(error instanceof Error ? error.message : "Invalid YAML format");
    }
  };

  return (
    <EditorPageWrapper
      headerPrefixAction={
        <ViewModeToggle
          viewMode={toAgentViewMode(viewMode)}
          onViewModeChange={(m) => setViewMode(toAgenticViewMode(m))}
          validationError={validationError}
        />
      }
      pathb64={pathb64}
      onSaved={refreshPreview}
      git={gitEnabled}
      customEditor={viewMode === AgenticViewMode.Form ? <AgenticFormWrapper /> : undefined}
      onChanged={(value) => {
        if (viewMode === AgenticViewMode.Editor) {
          validateContent(value);
        }
      }}
      preview={<PreviewSection pathb64={pathb64} previewKey={previewKey} />}
    />
  );
};

export default AgenticAnalyticsEditor;

const AgenticFormWrapper = () => {
  const { state, actions } = useFileEditorContext();
  const content = state.content;

  const data = useMemo(() => {
    try {
      if (!content) return undefined;
      const parsed = YAML.parse(content) as AgenticYamlData;
      return yamlToForm(parsed);
    } catch (error) {
      console.error("Failed to parse YAML content to form data:", error);
      return undefined;
    }
  }, [content]);

  const onChange = useMemo(
    () =>
      debounce((yamlData: AgenticYamlData) => {
        try {
          const yamlContent = YAML.stringify(yamlData, { indent: 2, lineWidth: 0 });
          actions.setContent(yamlContent);
        } catch (error) {
          console.error("Failed to serialize form data to YAML:", error);
        }
      }, 500),
    [actions]
  );

  if (!data) return null;

  return <AgenticAnalyticsForm data={data} onChange={onChange} />;
};
