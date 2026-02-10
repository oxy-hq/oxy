import { useEffect, useRef } from "react";
import { useFieldArray, useFormContext } from "react-hook-form";
import useTopicFieldOptions from "@/hooks/api/useTopicFieldOptions";
import useTopicFiles from "@/hooks/api/useTopicFiles";
import type { WorkflowFormData } from "../../..";

export const useSemanticQueryFields = (index: number, basePath: string) => {
  const {
    register,
    control,
    watch,
    formState: { errors }
  } = useFormContext<WorkflowFormData>();

  const taskPath = `${basePath}.${index}`;
  // @ts-expect-error - Dynamic path for nested tasks
  const taskErrors = errors[basePath]?.[index];

  // Watch the topic value
  // @ts-expect-error - dynamic field path
  const topicValue = watch(`${taskPath}.topic`) as string | undefined;

  // Track previous topic to detect changes
  const prevTopicRef = useRef<string | undefined>(topicValue);

  // Fetch available topics
  const { topicFiles, isLoading: topicsLoading, error: topicsError } = useTopicFiles();

  // Fetch field options based on selected topic
  const {
    dimensions: dimensionOptions,
    measures: measureOptions,
    allFields: allFieldOptions,
    isLoading: fieldsLoading
  } = useTopicFieldOptions(topicValue);

  // Use useFieldArray for dynamic arrays - only for clearing on topic change
  const { replace: replaceDimensions } = useFieldArray({
    control,
    // @ts-expect-error - dynamic field path
    name: `${taskPath}.dimensions`
  });

  const { replace: replaceMeasures } = useFieldArray({
    control,
    // @ts-expect-error - dynamic field path
    name: `${taskPath}.measures`
  });

  const { replace: replaceFilters } = useFieldArray({
    control,
    // @ts-expect-error - dynamic field path
    name: `${taskPath}.filters`
  });

  const { replace: replaceOrders } = useFieldArray({
    control,
    // @ts-expect-error - dynamic field path
    name: `${taskPath}.orders`
  });

  const { replace: replaceTimeDimensions } = useFieldArray({
    control,
    // @ts-expect-error - dynamic field path
    name: `${taskPath}.time_dimensions`
  });

  // Clear fields when topic changes
  useEffect(() => {
    if (prevTopicRef.current !== undefined && topicValue !== prevTopicRef.current) {
      replaceDimensions([]);
      replaceMeasures([]);
      replaceFilters([]);
      replaceOrders([]);
      replaceTimeDimensions([]);
    }
    prevTopicRef.current = topicValue;
  }, [
    topicValue,
    replaceDimensions,
    replaceMeasures,
    replaceFilters,
    replaceOrders,
    replaceTimeDimensions
  ]);

  const topicItems = topicFiles.map((t) => ({
    value: t.value,
    label: t.label,
    searchText: t.searchText
  }));

  const dimensionItems = dimensionOptions.map((d) => ({
    value: d.value,
    label: d.label,
    searchText: d.searchText
  }));

  const measureItems = measureOptions.map((m) => ({
    value: m.value,
    label: m.label,
    searchText: m.searchText
  }));

  const allFieldItems = allFieldOptions.map((f) => ({
    value: f.value,
    label: f.label,
    searchText: f.searchText,
    dataType: f.dataType
  }));

  const dimensionItemsWithTypes = dimensionOptions.map((d) => ({
    value: d.value,
    label: d.label,
    type: d.dataType as "string" | "number" | "date" | "datetime" | "boolean" | undefined
  }));

  return {
    register,
    control,
    taskPath,
    taskErrors,
    topicValue,
    topicsLoading,
    topicsError,
    fieldsLoading,
    topicItems,
    dimensionItems,
    measureItems,
    allFieldItems,
    dimensionItemsWithTypes
  };
};
