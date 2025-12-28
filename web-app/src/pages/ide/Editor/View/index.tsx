import { useMemo, useEffect, useCallback } from "react";
import { parse } from "yaml";

import {
  useExecuteSemanticQuery,
  useCompileSemanticQuery,
  useViewDetails,
} from "@/hooks/api/useSemanticQuery";
import { buildSemanticQuery } from "../utils/queryBuilder";
import EditorPageWrapper from "../components/EditorPageWrapper";
import { useEditorContext } from "../contexts/useEditorContext";
import { useFileEditorContext } from "@/components/FileEditor/useFileEditorContext";

import SemanticQueryPanel from "../components/SemanticQueryPanel";
import { useSemanticQueryState } from "../hooks/useSemanticQueryState";
import { ViewData } from "../types";
import FieldsSelectionPanel from "./FieldsSelectionPanel";

const ViewPreview = () => {
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

  const viewName = useMemo(() => {
    try {
      const parsed = parse(state.content);
      if (parsed && parsed.name) {
        return parsed.name;
      }
    } catch {
      // ignore error
    }
    const fileName = state.fileName.split("/").pop() || state.fileName;
    return fileName.replace(/\.(yml|yaml)$/, "");
  }, [state.fileName, state.content]);

  const { data: viewDetails, isLoading: isViewLoading } =
    useViewDetails(viewName);

  const loading = isExecuting || isCompiling || isViewLoading;

  // Parse the view file content
  const viewData = useMemo<ViewData | null>(() => {
    if (!viewDetails) return null;
    return {
      name: viewDetails.name,
      description: viewDetails.description,
      datasource: viewDetails.datasource || "",
      table: viewDetails.table || "",
      dimensions: viewDetails.dimensions || [],
      measures: viewDetails.measures || [],
    };
  }, [viewDetails]);

  const getFullFieldName = useCallback(
    (field: string) => {
      if (!viewData) return field;
      return `${viewData.name}.${field}`;
    },
    [viewData],
  );

  const canExecuteQuery = useMemo(() => {
    if (!viewData) return false;
    if (selectedDimensions.length === 0 && selectedMeasures.length === 0)
      return false;
    return true;
  }, [viewData, selectedDimensions, selectedMeasures]);

  const handleExecuteQuery = () => {
    if (!viewData) return;

    const request = buildSemanticQuery({
      dimensions: selectedDimensions,
      measures: selectedMeasures,
      filters,
      variables,
      getFullFieldName,
    });

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

  useEffect(() => {
    if (!viewData || !canExecuteQuery) return;

    const request = buildSemanticQuery({
      dimensions: selectedDimensions,
      measures: selectedMeasures,
      filters,
      variables,
      getFullFieldName,
    });

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
    viewData,
    selectedDimensions,
    selectedMeasures,
    filters,
    variables,
    canExecuteQuery,
    compileSemanticQuery,
    setGeneratedSql,
    setSqlError,
    getFullFieldName,
  ]);

  const addFilter = () => {
    if (!viewData || viewData.dimensions.length === 0) return;
    addFilterState(viewData.dimensions[0].name);
  };

  const availableDimensions = useMemo(() => {
    if (!viewData) return [];
    return viewData.dimensions.map((d) => ({
      label: d.name,
      value: d.name,
    }));
  }, [viewData]);

  return (
    <div className="flex-1 flex flex-col min-h-0">
      <div className="flex-1 flex gap-4 min-h-0">
        {/* Left Sidebar - Tree Structure */}
        <FieldsSelectionPanel
          viewData={viewData}
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
          availableDimensions={availableDimensions}
          hasData={!!viewData}
        />
      </div>
    </div>
  );
};

const ViewEditor = () => {
  const { pathb64, isReadOnly, gitEnabled } = useEditorContext();

  return (
    <EditorPageWrapper
      pathb64={pathb64}
      readOnly={isReadOnly}
      git={gitEnabled}
      defaultDirection="horizontal"
      preview={<ViewPreview />}
    />
  );
};

export default ViewEditor;
