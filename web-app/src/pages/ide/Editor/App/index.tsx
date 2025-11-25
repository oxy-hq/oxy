import { useMemo, useState } from "react";
import { debounce } from "lodash";
import EditorPageWrapper from "../components/EditorPageWrapper";
import AppPreview from "@/components/AppPreview";
import { useEditorContext } from "../contexts/useEditorContext";
import { usePreviewRefresh } from "../usePreviewRefresh";
import { useEditorQueryInvalidation } from "../useEditorQueryInvalidation";
import { AppForm, AppFormData } from "@/components/app/AppForm";
import { useFileEditorContext } from "@/components/FileEditor/useFileEditorContext";
import YAML from "yaml";
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/shadcn/tabs";
import { Code, FileText, AlertCircle } from "lucide-react";
import {
  Tooltip,
  TooltipTrigger,
  TooltipContent,
} from "@/components/ui/shadcn/tooltip";

const AppEditor = () => {
  const { pathb64, isReadOnly, gitEnabled } = useEditorContext();
  const { previewKey, refreshPreview } = usePreviewRefresh();
  const { invalidateAppQueries } = useEditorQueryInvalidation();
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

  const handleSaved = () => {
    refreshPreview();
    invalidateAppQueries();
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
      onSaved={handleSaved}
      readOnly={isReadOnly}
      git={gitEnabled}
      customEditor={viewMode === "form" ? <AppFormWrapper /> : undefined}
      onChanged={(value) => {
        if (viewMode === "editor") {
          validateContent(value);
        }
      }}
      preview={
        <div className="flex-1 overflow-hidden">
          <AppPreview key={previewKey} appPath64={pathb64} />
        </div>
      }
    />
  );
};
export default AppEditor;

const AppFormWrapper = () => {
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
      const parsed = YAML.parse(content) as Partial<AppFormData>;
      return parsed;
    } catch (error) {
      console.error("Failed to parse YAML content to form data:", error);
      return undefined;
    }
  }, [content]);

  const onChange = useMemo(
    () =>
      debounce((formData: AppFormData) => {
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

  return <AppForm data={data} onChange={onChange} />;
};
