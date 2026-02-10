import { useMemo } from "react";
import { useTopicDetails } from "./useSemanticQuery";
import useTopicFiles from "./useTopicFiles";

export interface FieldOption {
  value: string; // "view_name.field_name"
  label: string; // "view_name.field_name"
  searchText: string; // For filtering
  description?: string;
  type: "dimension" | "measure";
  dataType?: string; // Dimension data type (e.g., "date", "datetime", "string", "number", "boolean")
  viewName: string;
  fieldName: string;
}

export interface TopicFieldOptions {
  dimensions: FieldOption[];
  measures: FieldOption[];
  allFields: FieldOption[];
  isLoading: boolean;
  error: Error | null;
}

export default function useTopicFieldOptions(topicName: string | undefined): TopicFieldOptions {
  const { topicFiles, isLoading: topicFilesLoading } = useTopicFiles();

  // Resolve topic name to file path
  const topicFilePath = useMemo(() => {
    if (!topicName || topicFiles.length === 0) return undefined;
    return topicFiles.find((t) => t.value === topicName)?.path;
  }, [topicName, topicFiles]);

  const filePathB64 = useMemo(() => {
    if (!topicFilePath) return undefined;
    return btoa(topicFilePath);
  }, [topicFilePath]);

  const { data, isLoading: detailsLoading, error } = useTopicDetails(filePathB64);

  const options = useMemo(() => {
    if (!data) {
      return { dimensions: [], measures: [], allFields: [] };
    }

    const dimensions: FieldOption[] = [];
    const measures: FieldOption[] = [];

    for (const view of data.views) {
      // Add dimensions
      for (const dim of view.dimensions) {
        const value = `${view.view_name}.${dim.name}`;
        dimensions.push({
          value,
          label: value,
          searchText: `${value} ${dim.description || ""} ${dim.type}`.toLowerCase(),
          description: dim.description,
          type: "dimension",
          dataType: dim.type,
          viewName: view.view_name,
          fieldName: dim.name
        });
      }

      // Add measures
      for (const measure of view.measures) {
        const value = `${view.view_name}.${measure.name}`;
        measures.push({
          value,
          label: value,
          searchText: `${value} ${measure.description || ""} ${measure.type}`.toLowerCase(),
          description: measure.description,
          type: "measure",
          viewName: view.view_name,
          fieldName: measure.name
        });
      }
    }

    return {
      dimensions,
      measures,
      allFields: [...dimensions, ...measures]
    };
  }, [data]);

  return {
    ...options,
    isLoading: topicFilesLoading || detailsLoading,
    error: error as Error | null
  };
}
