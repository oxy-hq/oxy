import { useMemo } from "react";
import { debounce } from "lodash";
import { useFileEditorContext } from "@/components/FileEditor/useFileEditorContext";
import YAML from "yaml";
import { AppForm, AppFormData } from "@/components/app/AppForm";

export const AppFormWrapper = () => {
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

  console.log("Rendering AppFormWrapper with data:", data);

  const onChange = useMemo(
    () =>
      debounce((formData: AppFormData) => {
        try {
          // Merge formData into originalContent:
          // - Keys defined in AppFormData take precedence from formData (even if null/undefined)
          // - Keys only in originalContent are preserved
          const mergedData = { ...originalContent };
          const appFormDataKeys: (keyof AppFormData)[] = ["tasks", "display"];
          for (const key of appFormDataKeys) {
            if (key in formData) {
              mergedData[key] = formData[key];
            } else {
              delete mergedData[key];
            }
          }
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
