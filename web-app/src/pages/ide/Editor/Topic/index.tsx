import { useMemo, useEffect } from "react";
import { parse as parseYAML } from "yaml";

import EditorPageWrapper from "../components/EditorPageWrapper";
import { useEditorContext } from "../contexts/useEditorContext";
import { useFileEditorContext } from "@/components/FileEditor/useFileEditorContext";
import {
  useExecuteSemanticQuery,
  useCompileSemanticQuery,
  useTopicDetails,
} from "@/hooks/api/useSemanticQuery";
import { SemanticQueryRequest } from "@/services/api/semantic";
import SemanticQueryPanel from "../components/SemanticQueryPanel";
import { useSemanticQueryState } from "../hooks/useSemanticQueryState";
import FieldsSelectionPanel from "./FieldsSelectionPanel";

const TopicPreview = () => {
  const { state } = useFileEditorContext();
  const {
    result,
    setResult,
    selectedDimensions,
    selectedMeasures,
    filters,
    variables,
    showSql,
    setShowSql,
    generatedSql,
    setGeneratedSql,
    sqlError,
    setSqlError,
    executionError,
    setExecutionError,
    addFilter: addFilterState,
    updateFilter,
    removeFilter,
    addVariable,
    updateVariable,
    removeVariable,
    toggleDimension,
    toggleMeasure,
  } = useSemanticQueryState();

  const { mutate: executeSemanticQuery, isPending: isExecuting } =
    useExecuteSemanticQuery();
  const { mutate: compileSemanticQuery, isPending: isCompiling } =
    useCompileSemanticQuery();
  const loading = isExecuting || isCompiling;

  const topicName = useMemo(() => {
    try {
      if (!state.content) return null;
      const parsed = parseYAML(state.content);
      return parsed.name;
    } catch (error) {
      console.error("Failed to parse topic file:", error);
      return null;
    }
  }, [state.content]);

  const {
    data: topicDetails,
    isLoading: isLoadingTopicDetails,
    isError: isTopicDetailsError,
  } = useTopicDetails(topicName);

  const topicData = useMemo(() => {
    if (!topicDetails?.topic) return null;
    return {
      name: topicDetails.topic.name,
      description: topicDetails.topic.description,
      views: topicDetails.topic.views || [],
      base_view: topicDetails.topic.base_view,
    };
  }, [topicDetails]);

  const viewsWithData = useMemo(() => {
    if (!topicDetails?.views) return [];
    return topicDetails.views.map((view) => ({
      viewName: view.view_name,
      name: view.name,
      description: view.description,
      datasource: view.datasource || "",
      table: view.table || "",
      dimensions: view.dimensions || [],
      measures: view.measures || [],
    }));
  }, [topicDetails]);

  const handleExecuteQuery = () => {
    if (viewsWithData.length === 0 || !topicData) return;

    const request: SemanticQueryRequest = {
      query: {
        topic: topicData.name,
        dimensions: selectedDimensions,
        measures: selectedMeasures,
        filters: filters.map((f) => ({
          field: f.field,
          op: f.operator,
          value: f.value,
        })),
        variables: variables.reduce(
          (acc, v) => {
            if (v.key) acc[v.key] = v.value;
            return acc;
          },
          {} as Record<string, unknown>,
        ),
      },
    };

    executeSemanticQuery(request, {
      onSuccess: (data) => {
        setResult(data);
        setExecutionError(null);
      },
      onError: (error) => {
        setResult([]);
        setExecutionError(error.message);
      },
    });
  };

  const addFilter = () => {
    if (viewsWithData.length === 0) return;
    const firstView = viewsWithData[0];
    if (firstView.dimensions.length === 0) return;
    addFilterState(`${firstView.viewName}.${firstView.dimensions[0].name}`);
  };

  // Check if we can execute a query - need at least one dimension/measure from primary view
  const canExecuteQuery = useMemo(() => {
    if (viewsWithData.length === 0) return false;
    if (selectedDimensions.length === 0 && selectedMeasures.length === 0)
      return false;

    return true;
  }, [viewsWithData, selectedDimensions, selectedMeasures]);

  useEffect(() => {
    if (!topicData || !canExecuteQuery) return;

    const request: SemanticQueryRequest = {
      query: {
        topic: topicData.name,
        dimensions: selectedDimensions,
        measures: selectedMeasures,
        filters: filters.map((f) => ({
          field: f.field,
          op: f.operator,
          value: f.value,
        })),
        variables: variables.reduce(
          (acc, v) => {
            if (v.key) acc[v.key] = v.value;
            return acc;
          },
          {} as Record<string, unknown>,
        ),
      },
    };

    compileSemanticQuery(request, {
      onSuccess: (data) => {
        setGeneratedSql(data.sql);
        setSqlError(null);
      },
      onError: (error) => {
        setGeneratedSql("");
        setSqlError(error.message);
      },
    });
  }, [
    topicData,
    selectedDimensions,
    selectedMeasures,
    filters,
    variables,
    canExecuteQuery,
    compileSemanticQuery,
    setGeneratedSql,
    setSqlError,
  ]);

  const availableDimensions = useMemo(() => {
    return viewsWithData.flatMap((view) =>
      view.dimensions.map((dim) => ({
        label: `${view.viewName}.${dim.name}`,
        value: `${view.viewName}.${dim.name}`,
      })),
    );
  }, [viewsWithData]);

  return (
    <div className="flex-1 flex flex-col overflow-hidden">
      <div className="flex-1 flex overflow-hidden">
        {/* Left Sidebar - Tree Structure */}
        <FieldsSelectionPanel
          topicData={topicData}
          viewsWithData={viewsWithData}
          isLoading={isLoadingTopicDetails}
          isError={isTopicDetailsError}
          selectedDimensions={selectedDimensions}
          selectedMeasures={selectedMeasures}
          toggleDimension={toggleDimension}
          toggleMeasure={toggleMeasure}
        />

        {/* Right Side - Results and SQL */}
        <SemanticQueryPanel
          result={result}
          showSql={showSql}
          setShowSql={setShowSql}
          generatedSql={generatedSql}
          sqlError={sqlError}
          executionError={executionError}
          filters={filters}
          variables={variables}
          onAddFilter={addFilter}
          onUpdateFilter={updateFilter}
          onRemoveFilter={removeFilter}
          onAddVariable={addVariable}
          onUpdateVariable={updateVariable}
          onRemoveVariable={removeVariable}
          onExecuteQuery={handleExecuteQuery}
          loading={loading}
          canExecuteQuery={canExecuteQuery}
          disabledMessage="Select at least one dimension or measure from the base view"
          availableDimensions={availableDimensions}
          hasData={viewsWithData.length > 0}
        />
      </div>
    </div>
  );
};

const TopicEditor = () => {
  const { pathb64, isReadOnly, gitEnabled } = useEditorContext();

  return (
    <EditorPageWrapper
      pathb64={pathb64}
      readOnly={isReadOnly}
      git={gitEnabled}
      defaultDirection="horizontal"
      preview={<TopicPreview />}
    />
  );
};

export default TopicEditor;
