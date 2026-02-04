import { createContext, type ReactNode, useCallback, useContext, useEffect, useMemo } from "react";
import { useCompileSemanticQuery, useExecuteSemanticQuery } from "@/hooks/api/useSemanticQuery";
import type { Variable } from "../components/SemanticQueryPanel";
import { useSemanticQueryState } from "../hooks/useSemanticQueryState";
import type { Field, Filter, Order } from "../types";
import { buildSemanticQuery } from "../utils/queryBuilder";

type SemanticExplorerContextType = {
  // Data
  dataLoading: boolean;
  loadingError?: string;
  loading: boolean;
  refetchData?: () => void;

  // Selection state
  selectedDimensions: string[];
  selectedMeasures: string[];
  toggleDimension: (dimension: string) => void;
  toggleMeasure: (measure: string) => void;

  // Query results
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  result: any[][];
  showSql: boolean;
  setShowSql: (show: boolean) => void;
  generatedSql: string;
  sqlError: string | null;
  executionError: string | null;

  // Filters
  filters: Filter[];
  onAddFilter: () => void;
  onUpdateFilter: (index: number, updates: Filter) => void;
  onRemoveFilter: (index: number) => void;

  // Orders
  orders: Order[];
  onAddOrder: () => void;
  onUpdateOrder: (index: number, updates: Order) => void;
  onRemoveOrder: (index: number) => void;

  // Variables
  variables: Variable[];
  onAddVariable: () => void;
  onUpdateVariable: (index: number, updates: Partial<Variable>) => void;
  onRemoveVariable: (index: number) => void;

  // Actions
  onExecuteQuery: () => void;
  availableDimensions: Field[];
  availableMeasures: Field[];
  setGeneratedSql: (sql: string) => void;
  setSqlError: (error: string | null) => void;
  canExecuteQuery: boolean;
  resultFile?: string;
};

const SemanticExplorerContext = createContext<SemanticExplorerContextType | null>(null);

type SemanticExplorerProviderProps = {
  children: ReactNode;
  dataLoading: boolean;
  loadingError?: string;
  refetchData?: () => void;
  availableDimensions: Field[];
  availableMeasures: Field[];
  canExecuteQuery: boolean;
  onAddOrderDefault?: () => void;
  topic?: string;
};

export const SemanticExplorerProvider = ({
  topic,
  children,
  dataLoading,
  loadingError,
  refetchData,
  availableDimensions,
  availableMeasures,
  canExecuteQuery,
  onAddOrderDefault
}: SemanticExplorerProviderProps) => {
  const {
    result,
    setResult,
    resultFile,
    setResultFile,
    selectedDimensions,
    selectedMeasures,
    filters,
    orders,
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
    addOrder: addOrderState,
    updateOrder,
    removeOrder,
    addVariable,
    updateVariable,
    removeVariable,
    toggleDimension,
    toggleMeasure
  } = useSemanticQueryState();

  const { mutate: executeSemanticQuery, isPending: isExecuting } = useExecuteSemanticQuery();
  const { mutate: compileSemanticQuery, isPending: isCompiling } = useCompileSemanticQuery();

  const loading = isExecuting || isCompiling || dataLoading;

  // Auto-compile query when selection changes
  useEffect(() => {
    if (!canExecuteQuery || (selectedDimensions.length === 0 && selectedMeasures.length === 0))
      return;

    const request = buildSemanticQuery({
      topic,
      dimensions: selectedDimensions,
      measures: selectedMeasures,
      filters,
      orders,
      variables
    });

    compileSemanticQuery(request, {
      onSuccess: (data) => {
        setGeneratedSql(data.sql);
        setSqlError(null);
      },
      onError: (error) => {
        setGeneratedSql("");
        setSqlError(error.message);
      }
    });
  }, [
    canExecuteQuery,
    selectedDimensions,
    selectedMeasures,
    filters,
    orders,
    variables,
    compileSemanticQuery,
    setGeneratedSql,
    setSqlError,
    topic
  ]);

  const handleExecuteQuery = useCallback(() => {
    if (!canExecuteQuery) return;

    const request = buildSemanticQuery({
      topic,
      dimensions: selectedDimensions,
      measures: selectedMeasures,
      filters,
      orders,
      variables
    });

    executeSemanticQuery(request, {
      onSuccess: (data) => {
        setResultFile((data as { file_name: string }).file_name);
        setResult([]);
        setExecutionError(null);
      },
      onError: (error) => {
        setResult([]);
        setResultFile(undefined);
        setExecutionError(error.message);
      }
    });
  }, [
    canExecuteQuery,
    topic,
    selectedDimensions,
    selectedMeasures,
    filters,
    orders,
    variables,
    executeSemanticQuery,
    setResult,
    setResultFile,
    setExecutionError
  ]);

  const addFilter = useCallback(() => {
    if (availableDimensions.length > 0) {
      addFilterState(availableDimensions[0].fullName);
    }
  }, [availableDimensions, addFilterState]);

  const addOrder = useCallback(() => {
    if (onAddOrderDefault) {
      onAddOrderDefault();
    } else if (selectedDimensions.length > 0) {
      addOrderState(selectedDimensions[0]);
    } else if (selectedMeasures.length > 0) {
      addOrderState(selectedMeasures[0]);
    }
  }, [onAddOrderDefault, selectedDimensions, selectedMeasures, addOrderState]);

  const value = useMemo(
    () => ({
      dataLoading,
      loadingError,
      loading,
      refetchData,
      selectedDimensions,
      selectedMeasures,
      toggleDimension,
      toggleMeasure,
      result,
      showSql,
      setShowSql,
      generatedSql,
      sqlError,
      executionError,
      filters,
      orders,
      variables,
      onAddFilter: addFilter,
      onUpdateFilter: updateFilter,
      onRemoveFilter: removeFilter,
      onAddOrder: addOrder,
      onUpdateOrder: updateOrder,
      onRemoveOrder: removeOrder,
      onAddVariable: addVariable,
      onUpdateVariable: updateVariable,
      onRemoveVariable: removeVariable,
      onExecuteQuery: handleExecuteQuery,
      availableDimensions,
      setSqlError,
      setGeneratedSql,
      canExecuteQuery,
      availableMeasures,
      setResult,
      resultFile,
      setResultFile
    }),
    [
      dataLoading,
      loadingError,
      loading,
      refetchData,
      selectedDimensions,
      selectedMeasures,
      toggleDimension,
      toggleMeasure,
      result,
      showSql,
      setShowSql,
      generatedSql,
      sqlError,
      executionError,
      filters,
      orders,
      variables,
      addFilter,
      updateFilter,
      removeFilter,
      addOrder,
      updateOrder,
      removeOrder,
      addVariable,
      updateVariable,
      removeVariable,
      handleExecuteQuery,
      availableDimensions,
      setSqlError,
      setGeneratedSql,
      canExecuteQuery,
      availableMeasures,
      setResult,
      resultFile,
      setResultFile
    ]
  );

  return (
    <SemanticExplorerContext.Provider value={value}>{children}</SemanticExplorerContext.Provider>
  );
};

export const useSemanticExplorerContext = () => {
  const context = useContext(SemanticExplorerContext);
  if (!context) {
    throw new Error("useSemanticExplorerContext must be used within SemanticExplorerProvider");
  }
  return context;
};
