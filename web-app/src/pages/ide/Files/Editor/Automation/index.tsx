import { debounce } from "lodash";
import { useMemo, useState } from "react";
import { useSearchParams } from "react-router-dom";
import YAML from "yaml";
import {
  AutomationForm,
  type AutomationFormData,
  type TaskFormData
} from "@/components/automation/AutomationForm";
import { useFileEditorContext } from "@/components/FileEditor/useFileEditorContext";
import { Automation } from "@/pages/automation";
import { useFilesContext } from "../../FilesContext";
import { FilesSubViewMode } from "../../FilesSidebar/constants";
import EditorPageWrapper from "../components/EditorPageWrapper";
import { useEditorContext } from "../contexts/useEditorContext";
import { usePreviewRefresh } from "../usePreviewRefresh";
import ModeSwitcher from "./components/ModeSwitcher";
import { AutomationViewMode } from "./components/types";

const AutomationEditor = () => {
  const { pathb64, gitEnabled } = useEditorContext();
  const { refreshPreview, previewKey } = usePreviewRefresh();
  const { filesSubViewMode } = useFilesContext();

  const [searchParams] = useSearchParams();
  const runId = searchParams.get("run") || undefined;

  const defaultViewMode =
    filesSubViewMode === FilesSubViewMode.OBJECTS
      ? AutomationViewMode.Form
      : AutomationViewMode.Output;

  const [viewMode, setViewMode] = useState<AutomationViewMode>(defaultViewMode);

  return (
    <EditorPageWrapper
      headerPrefixAction={<ModeSwitcher viewMode={viewMode} onViewModeChange={setViewMode} />}
      pathb64={pathb64}
      onSaved={refreshPreview}
      customEditor={viewMode === AutomationViewMode.Form ? <AutomationFormWrapper /> : undefined}
      git={gitEnabled}
      preview={
        <Automation
          key={previewKey + runId}
          pathb64={pathb64}
          runId={runId}
          direction='vertical'
          hideHeader
        />
      }
      previewOnly={viewMode === AutomationViewMode.Output}
    />
  );
};
export default AutomationEditor;

// Convert filters from YAML map {key: value} to form array [{key, value}]
const filtersMapToArray = (filters: unknown): Array<{ key: string; value: string }> | undefined => {
  if (!filters || typeof filters !== "object" || Array.isArray(filters)) return undefined;
  return Object.entries(filters as Record<string, string>).map(([key, value]) => ({
    key,
    value
  }));
};

// Convert filters from form array [{key, value}] to YAML map {key: value}
const filtersArrayToMap = (filters: unknown): Record<string, string> | undefined => {
  if (!Array.isArray(filters) || filters.length === 0) return undefined;
  const map: Record<string, string> = {};
  for (const f of filters as Array<{ key?: string; value?: string }>) {
    if (f.key) map[f.key] = f.value ?? "";
  }
  return Object.keys(map).length > 0 ? map : undefined;
};

const transformTasksForForm = (tasks: TaskFormData[]): TaskFormData[] =>
  tasks.map((task) => {
    const converted = filtersMapToArray(task.filters);
    return converted !== undefined ? { ...task, filters: converted } : task;
  });

const transformTasksForYaml = (tasks: TaskFormData[]): TaskFormData[] =>
  tasks.map((task) => {
    const converted = filtersArrayToMap(task.filters);
    return { ...task, filters: converted };
  });

const AutomationFormWrapper = () => {
  const { state, actions } = useFileEditorContext();

  const content = state.content;

  const data = useMemo(() => {
    try {
      if (!content) return undefined;
      const parsed = YAML.parse(content) as Partial<AutomationFormData>;
      return {
        ...parsed,
        tasks: Array.isArray(parsed.tasks) ? transformTasksForForm(parsed.tasks) : parsed.tasks,
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
      debounce((formData: AutomationFormData) => {
        try {
          const prepared = {
            ...formData,
            tasks: Array.isArray(formData.tasks)
              ? transformTasksForYaml(formData.tasks)
              : formData.tasks
          };
          const yamlContent = YAML.stringify(prepared, {
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

  return <AutomationForm data={data} onChange={onChange} />;
};
