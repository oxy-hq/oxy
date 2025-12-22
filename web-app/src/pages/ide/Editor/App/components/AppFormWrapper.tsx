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
