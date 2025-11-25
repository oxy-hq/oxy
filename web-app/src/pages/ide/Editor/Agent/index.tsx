import { useMemo, useState } from "react";
import { debounce } from "lodash";
import EditorPageWrapper from "../components/EditorPageWrapper";
import AgentPreview from "./Preview";
import {
  ToggleGroup,
  ToggleGroupItem,
} from "@/components/ui/shadcn/toggle-group";
import AgentTests from "./Tests";
import { Button } from "@/components/ui/shadcn/button";
import { BrushCleaning, Play, Code, FileText, AlertCircle } from "lucide-react";
import useAgent from "@/hooks/api/agents/useAgent";
import useTests from "@/stores/useTests";
import useAgentThreadStore from "@/stores/useAgentThread";
import { useEditorContext } from "../contexts/useEditorContext";
import { useEditorQueryInvalidation } from "../useEditorQueryInvalidation";
import { usePreviewRefresh } from "../usePreviewRefresh";
import { AgentForm, AgentFormData } from "@/components/agent/AgentForm";
import { useFileEditorContext } from "@/components/FileEditor/useFileEditorContext";
import YAML from "yaml";
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/shadcn/tabs";
import {
  Tooltip,
  TooltipTrigger,
  TooltipContent,
} from "@/components/ui/shadcn/tooltip";

const AgentEditor = () => {
  const { pathb64, project, branchName, isReadOnly, gitEnabled } =
    useEditorContext();
  const { previewKey, refreshPreview } = usePreviewRefresh();
  const [selected, setSelected] = useState<string>("preview");
  const [viewMode, setViewMode] = useState<"editor" | "form">("editor");
  const [validationError, setValidationError] = useState<string | null>(null);
  const { setMessages } = useAgentThreadStore();
  const { invalidateAgentQueries } = useEditorQueryInvalidation();

  const { data: agent, isLoading } = useAgent(pathb64);
  const { runTest } = useTests();

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

  const handleRunAllTests = () => {
    if (isLoading) return;
    const tests = agent?.tests || [];
    for (const [index] of tests.entries()) {
      runTest(project.id, branchName, pathb64, index);
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
      onSaved={handleSaved}
      readOnly={isReadOnly}
      git={gitEnabled}
      customEditor={viewMode === "form" ? <AgentFormWrapper /> : undefined}
      onChanged={(value) => {
        if (viewMode === "editor") {
          validateContent(value);
        }
      }}
      preview={
        <div className="flex-1 overflow-hidden flex flex-col">
          <div className="flex justify-between p-4 flex-shrink-0 relative z-10 bg-background">
            <ToggleGroup
              size="sm"
              value={selected}
              onValueChange={setSelected}
              type="single"
            >
              <ToggleGroupItem value="preview" aria-label="Preview">
                Preview
              </ToggleGroupItem>
              <ToggleGroupItem value="test" aria-label="Test">
                Test
              </ToggleGroupItem>
            </ToggleGroup>
            {selected === "test" && (
              <Button
                size="sm"
                variant="ghost"
                onClick={handleRunAllTests}
                title={"Run all tests"}
              >
                <Play className="w-4 h-4" />
                Run all tests
              </Button>
            )}
            {selected === "preview" && (
              <Button
                size="sm"
                variant={"ghost"}
                onClick={() => {
                  setMessages(pathb64, []);
                }}
              >
                <BrushCleaning className="w-4 h-4" />
                Clean
              </Button>
            )}
          </div>

          <div className="flex-1 overflow-auto">
            {selected === "preview" ? (
              <AgentPreview key={previewKey} agentPathb64={pathb64} />
            ) : (
              <AgentTests key={previewKey} agentPathb64={pathb64} />
            )}
          </div>
        </div>
      }
    />
  );
};
export default AgentEditor;

const AgentFormWrapper = () => {
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

  return <AgentForm data={data} onChange={onChange} />;
};
