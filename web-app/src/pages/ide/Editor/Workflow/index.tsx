import { useMemo, useState } from "react";
import { debounce } from "lodash";
import EditorPageWrapper from "../components/EditorPageWrapper";
import {
  WorkflowForm,
  WorkflowFormData,
} from "@/components/workflow/WorkflowForm";
import { WorkflowPreview } from "@/components/workflow/WorkflowPreview";
import { usePreviewRefresh } from "../usePreviewRefresh";
import YAML from "yaml";
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/shadcn/tabs";
import { Code, FileText, AlertCircle } from "lucide-react";
import { useFileEditorContext } from "@/components/FileEditor/useFileEditorContext";
import { useEditorContext } from "../contexts/useEditorContext";
import {
  Tooltip,
  TooltipTrigger,
  TooltipContent,
} from "@/components/ui/shadcn/tooltip";
import { useSearchParams } from "react-router-dom";

const WorkflowEditor = () => {
  const { pathb64, isReadOnly, gitEnabled } = useEditorContext();
  const { refreshPreview, previewKey } = usePreviewRefresh();
  const [searchParams] = useSearchParams();
  const runId = searchParams.get("run") || undefined;
  const [viewMode, setViewMode] = useState<"editor" | "form">("editor");
  const [validationError, setValidationError] = useState<string | null>(null);

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

  return (
    <EditorPageWrapper
      headerActions={
        <>
          {validationError ? (
            <Tooltip>
              <TooltipTrigger asChild>
                <AlertCircle className="w-4 h-4 cursor-pointer text-destructive" />
              </TooltipTrigger>
              <TooltipContent className="max-w-md">
                <p className="text-sm">{validationError}</p>
              </TooltipContent>
            </Tooltip>
          ) : (
            <Tabs
              value={viewMode}
              onValueChange={(value: string) => {
                if (value === "form" || value === "editor") {
                  setViewMode(value);
                }
              }}
            >
              <TabsList className="h-8">
                <TabsTrigger value="editor" className="h-6 px-2">
                  <Code className="w-4 h-4" />
                </TabsTrigger>
                <TabsTrigger value="form" className="h-6 px-2">
                  <FileText className="w-4 h-4" />
                </TabsTrigger>
              </TabsList>
            </Tabs>
          )}
        </>
      }
      pathb64={pathb64}
      readOnly={isReadOnly}
      onSaved={refreshPreview}
      preview={
        <WorkflowPreview
          key={previewKey + runId}
          pathb64={pathb64}
          runId={runId}
          direction="vertical"
        />
      }
      customEditor={viewMode === "form" ? <WorkflowFormWrapper /> : undefined}
      git={gitEnabled}
      onChanged={(value) => {
        if (viewMode === "editor") {
          validateContent(value);
        }
      }}
    />
  );
};
export default WorkflowEditor;

const WorkflowFormWrapper = () => {
  const { state, actions } = useFileEditorContext();

  const content = state.content;

  const originalContent = useMemo(() => {
    try {
      if (!content) return undefined;
      return YAML.parse(content);
    } catch (error) {
      console.error("Failed to parse original YAML content:", error);
      return undefined;
    }
  }, [content]);

  const data = useMemo(() => {
    try {
      if (!content) return undefined;
      const parsed = YAML.parse(content) as Partial<WorkflowFormData>;
      console.log("Parsed YAML content:", parsed);
      return {
        ...parsed,
        variables:
          parsed.variables && typeof parsed.variables === "object"
            ? JSON.stringify(parsed.variables, null, 2)
            : parsed.variables?.toString() || "",
      };
    } catch (error) {
      console.error("Failed to parse YAML content to form data:", error);
      return undefined;
    }
  }, [content]);

  const onChange = useMemo(
    () =>
      debounce((formData: WorkflowFormData) => {
        try {
          const mergedData = {
            ...originalContent,
            ...formData,
          };

          const yamlContent = YAML.stringify(mergedData, {
            indent: 2,
            lineWidth: 0,
          });
          actions.setContent(yamlContent);
        } catch (error) {
          console.error("Failed to serialize form data to YAML:", error);
        }
      }, 500),
    [actions, originalContent],
  );

  if (!data) return null;

  return <WorkflowForm data={data} onChange={onChange} />;
};
