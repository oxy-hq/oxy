import { useMemo, useState } from "react";
import YAML from "yaml";
import { useFileEditorContext } from "@/components/FileEditor/useFileEditorContext";
import { useFilesContext } from "../../FilesContext";
import { FilesSubViewMode } from "../../FilesSidebar/constants";
import EditorPageWrapper from "../components/EditorPageWrapper";
import { useEditorContext } from "../contexts/useEditorContext";
import ViewModeToggle from "./components/ViewModeToggle";
import RunSection from "./RunSection";
import TestFileForm, { type TestFileFormData } from "./TestFileForm";
import { TestFileViewMode } from "./types";

const TestFileEditor = () => {
  const { pathb64, gitEnabled } = useEditorContext();
  const { filesSubViewMode } = useFilesContext();

  const defaultViewMode =
    filesSubViewMode === FilesSubViewMode.OBJECTS ? TestFileViewMode.Form : TestFileViewMode.Editor;

  const [viewMode, setViewMode] = useState<TestFileViewMode>(defaultViewMode);
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
      git={gitEnabled}
      customEditor={viewMode === TestFileViewMode.Form ? <TestFileFormWrapper /> : undefined}
      onChanged={(value) => {
        if (viewMode === TestFileViewMode.Editor) {
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
