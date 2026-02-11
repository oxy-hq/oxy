import { debounce } from "lodash";
import { useEffect, useMemo, useRef, useState } from "react";
import { useLocation, useNavigate, useSearchParams } from "react-router-dom";
import YAML from "yaml";
import { useFileEditorContext } from "@/components/FileEditor/useFileEditorContext";
import { useListWorkflowRuns } from "@/components/workflow/useWorkflowRun";
import { WorkflowForm, type WorkflowFormData } from "@/components/workflow/WorkflowForm";
import { WorkflowPreview } from "@/components/workflow/WorkflowPreview";
import { decodeBase64 } from "@/libs/encoding";
import { useFilesContext } from "../../FilesContext";
import { FilesSubViewMode } from "../../FilesSidebar/constants";
import { useEditorContext } from "../contexts/useEditorContext";
import { usePreviewRefresh } from "../usePreviewRefresh";
import { WorkflowViewMode } from "./components/types";
import WorkflowEditorView from "./components/WorkflowEditorView";
import WorkflowOutputView from "./components/WorkflowOutputView";

const WorkflowEditor = () => {
  const { pathb64, isReadOnly, gitEnabled } = useEditorContext();
  const { refreshPreview, previewKey } = usePreviewRefresh();
  const [searchParams] = useSearchParams();
  const navigate = useNavigate();
  const location = useLocation();
  const runIdFromParams = searchParams.get("run") || undefined;
  const { filesSubViewMode } = useFilesContext();

  // Default to Form for object mode (GUI editor), Output for file mode
  const defaultViewMode =
    filesSubViewMode === FilesSubViewMode.OBJECTS ? WorkflowViewMode.Form : WorkflowViewMode.Output;

  const [viewMode, setViewMode] = useState<WorkflowViewMode>(defaultViewMode);

  const [validationError, setValidationError] = useState<string | null>(null);
  const hasNavigatedRef = useRef(false);

  // Get the workflow path for fetching runs
  const workflowPath = useMemo(() => decodeBase64(pathb64 ?? ""), [pathb64]);

  // Reset navigation flag when workflow path changes
  // biome-ignore lint/correctness/useExhaustiveDependencies: <explanation>
  useEffect(() => {
    hasNavigatedRef.current = false;
  }, [workflowPath]);

  // Fetch the most recent workflow run
  const { data: runsData, isPending: isRunsLoading } = useListWorkflowRuns(workflowPath, {
    pageIndex: 0,
    pageSize: 1
  });

  // Determine the run ID to use
  const runId = useMemo(() => {
    if (runIdFromParams) return runIdFromParams;
    // Auto-load the most recent run if available
    if (runsData?.items && runsData.items.length > 0) {
      return runsData.items[0].run_index.toString();
    }
    return undefined;
  }, [runIdFromParams, runsData]);

  // Auto-navigate to the most recent run when in output mode and no run is specified
  useEffect(() => {
    if (
      viewMode === WorkflowViewMode.Output &&
      !runIdFromParams &&
      runId &&
      !isRunsLoading &&
      !hasNavigatedRef.current
    ) {
      hasNavigatedRef.current = true;
      const newSearchParams = new URLSearchParams(location.search);
      newSearchParams.set("run", runId);
      navigate(
        {
          pathname: location.pathname,
          search: newSearchParams.toString()
        },
        { replace: true }
      );
    }
  }, [
    viewMode,
    runIdFromParams,
    runId,
    isRunsLoading,
    navigate,
    location.pathname,
    location.search
  ]);

  const validateContent = (value: string) => {
    try {
      YAML.parse(value);
      setValidationError(null);
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : "Invalid YAML format";
      setValidationError(errorMessage);
    }
  };

  // Render full-screen output mode
  if (viewMode === WorkflowViewMode.Output) {
    return (
      <WorkflowOutputView
        viewMode={viewMode}
        onViewModeChange={setViewMode}
        workflowPath={workflowPath}
        pathb64={pathb64}
        runId={runId}
      />
    );
  }

  // Render editor or form mode with EditorPageWrapper
  return (
    <WorkflowEditorView
      viewMode={viewMode}
      onViewModeChange={setViewMode}
      workflowPath={workflowPath}
      validationError={validationError}
      pathb64={pathb64}
      isReadOnly={isReadOnly}
      onSaved={refreshPreview}
      customEditor={viewMode === WorkflowViewMode.Form ? <WorkflowFormWrapper /> : undefined}
      gitEnabled={gitEnabled}
      onChanged={(value) => {
        if (viewMode === WorkflowViewMode.Editor) {
          validateContent(value);
        }
      }}
      preview={
        <WorkflowPreview
          key={previewKey + runId}
          pathb64={pathb64}
          runId={runId}
          direction='vertical'
        />
      }
    />
  );
};
export default WorkflowEditor;

const WorkflowFormWrapper = () => {
  const { state, actions } = useFileEditorContext();

  const content = state.content;

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
            : parsed.variables?.toString() || ""
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
          const yamlContent = YAML.stringify(formData, {
            indent: 2,
            lineWidth: 0
          });
          actions.setContent(yamlContent);
        } catch (error) {
          console.error("Failed to serialize form data to YAML:", error);
        }
      }, 500),
    [actions]
  );

  if (!data) return null;

  return <WorkflowForm data={data} onChange={onChange} />;
};
