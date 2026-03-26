import { useMemo, useState } from "react";
import YAML from "yaml";
import { useFileEditorContext } from "@/components/FileEditor/useFileEditorContext";
import { useFilesContext } from "../../FilesContext";
import { FilesSubViewMode } from "../../FilesSidebar/constants";
import ViewModeToggle from "../Agent/components/ViewModeToggle";
import { AgentViewMode } from "../Agent/types";
import EditorPageWrapper from "../components/EditorPageWrapper";
import { useEditorContext } from "../contexts/useEditorContext";
import RunSection from "./RunSection";
import TestFileForm, { type TestFileFormData } from "./TestFileForm";

const TestFileEditor = () => {
  const { pathb64, isReadOnly, gitEnabled } = useEditorContext();
  const { filesSubViewMode } = useFilesContext();

  const defaultViewMode =
    filesSubViewMode === FilesSubViewMode.OBJECTS ? AgentViewMode.Form : AgentViewMode.Editor;

  const [viewMode, setViewMode] = useState<AgentViewMode>(defaultViewMode);
  const [validationError, setValidationError] = useState<string | null>(null);

  const validateContent = (value: string) => {
    try {
      YAML.parse(value);
      setValidationError(null);
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : "Invalid YAML format";
      setValidationError(errorMessage);
    }
  };

  return (
    <EditorPageWrapper
      headerPrefixAction={
        <ViewModeToggle
          viewMode={viewMode}
          onViewModeChange={setViewMode}
          validationError={validationError}
        />
      }
      pathb64={pathb64}
      readOnly={isReadOnly}
      git={gitEnabled}
      customEditor={viewMode === AgentViewMode.Form ? <TestFileFormWrapper /> : undefined}
      onChanged={(value) => {
        if (viewMode === AgentViewMode.Editor) {
          validateContent(value);
        }
      }}
      preview={<RunSection pathb64={pathb64} />}
    />
  );
};

export default TestFileEditor;

const TestFileFormWrapper = () => {
  const { state, actions } = useFileEditorContext();

  const content = state.content;

  const data = useMemo(() => {
    try {
      if (!content) return undefined;
      const parsed = YAML.parse(content) as Partial<TestFileFormData>;
      return parsed;
    } catch (error) {
      console.error("Failed to parse YAML content to form data:", error);
      return undefined;
    }
  }, [content]);

  const onChange = useMemo(
    () => (formData: TestFileFormData) => {
      try {
        const yamlContent = YAML.stringify(formData, {
          indent: 2,
          lineWidth: 0
        });
        actions.setContent(yamlContent);
      } catch (error) {
        console.error("Failed to serialize form data to YAML:", error);
      }
    },
    [actions]
  );

  if (!data) {
    return (
      <div className='flex h-full items-center justify-center text-muted-foreground'>
        Failed to parse test file
      </div>
    );
  }

  return <TestFileForm data={data} onChange={onChange} />;
};
