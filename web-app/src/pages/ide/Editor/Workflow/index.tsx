import { useMemo, useState, useEffect, useRef } from "react";
import { debounce } from "lodash";
import {
  WorkflowForm,
  WorkflowFormData,
} from "@/components/workflow/WorkflowForm";
import { usePreviewRefresh } from "../usePreviewRefresh";
import YAML from "yaml";
import { useFileEditorContext } from "@/components/FileEditor/useFileEditorContext";
import { useEditorContext } from "../contexts/useEditorContext";
import { useSearchParams, useNavigate, useLocation } from "react-router-dom";
import { useListWorkflowRuns } from "@/components/workflow/useWorkflowRun";
import WorkflowOutputView from "./components/WorkflowOutputView";
import WorkflowEditorView from "./components/WorkflowEditorView";
import { WorkflowViewMode } from "./components/types";
import { WorkflowPreview } from "@/components/workflow/WorkflowPreview";

const WorkflowEditor = () => {
  const { pathb64, isReadOnly, gitEnabled } = useEditorContext();
  const { refreshPreview } = usePreviewRefresh();
  const [searchParams] = useSearchParams();
  const navigate = useNavigate();
  const location = useLocation();
  const runIdFromParams = searchParams.get("run") || undefined;
  const [viewMode, setViewMode] = useState<WorkflowViewMode>(
    WorkflowViewMode.Output,
  );
  const [validationError, setValidationError] = useState<string | null>(null);
  const hasNavigatedRef = useRef(false);

  // Get the workflow path for fetching runs
  const workflowPath = useMemo(() => atob(pathb64 ?? ""), [pathb64]);

  // Reset navigation flag when workflow path changes
  useEffect(() => {
    hasNavigatedRef.current = false;
  }, [workflowPath]);

  // Fetch the most recent workflow run
  const { data: runsData, isPending: isRunsLoading } = useListWorkflowRuns(
    workflowPath,
    {
      pageIndex: 0,
      pageSize: 1,
    },
  );

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
          search: newSearchParams.toString(),
        },
        { replace: true },
      );
    }
  }, [
    viewMode,
    runIdFromParams,
    runId,
    isRunsLoading,
    navigate,
    location.pathname,
    location.search,
  ]);

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
      customEditor={
        viewMode === WorkflowViewMode.Form ? <WorkflowFormWrapper /> : undefined
      }
      gitEnabled={gitEnabled}
      onChanged={(value) => {
        if (viewMode === WorkflowViewMode.Editor) {
          validateContent(value);
        }
      }}
      preview={
        <WorkflowPreview pathb64={pathb64} runId={runId} direction="vertical" />
      }
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
