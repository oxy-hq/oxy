import { createContext, type ReactNode, useCallback, useContext, useEffect, useMemo } from "react";
import { useCompileSemanticQuery, useExecuteSemanticQuery } from "@/hooks/api/useSemanticQuery";
import type { TimeDimension } from "@/types/artifact";
import type { Variable } from "../components/SemanticQueryPanel";
import { useSemanticQueryState } from "../hooks/useSemanticQueryState";
import type { Field, Filter, Order } from "../types";
import { buildSemanticQuery } from "../utils/queryBuilder";

type SemanticExplorerContextType = {
  dataLoading: boolean;
  loadingError?: string;
  loading: boolean;
  sqlLoading: boolean;
  executeLoading: boolean;
  refetchData?: () => void;

  // Selection state
  selectedDimensions: string[];
  selectedMeasures: string[];
  toggleDimension: (dimension: string) => void;
  toggleMeasure: (measure: string) => void;
  setSelectedDimensions: (dimensions: string[]) => void;
  setSelectedMeasures: (measures: string[]) => void;

  // Query results
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  result: any[][];
  showSql: boolean;
  setShowSql: (show: boolean) => void;
  generatedSql: string;
  sqlError: string | null;
  executionError: string | null;

  filters: Filter[];
  onAddFilter: () => void;
  onUpdateFilter: (index: number, updates: Filter) => void;
  onRemoveFilter: (index: number) => void;

  orders: Order[];
  onAddOrder: () => void;
  onUpdateOrder: (index: number, updates: Order) => void;
  onRemoveOrder: (index: number) => void;

  variables: Variable[];
  onAddVariable: () => void;
  onUpdateVariable: (index: number, updates: Partial<Variable>) => void;
  onRemoveVariable: (index: number) => void;

  timeDimensions: TimeDimension[];
  onAddTimeDimension: (initialValues?: Partial<TimeDimension>) => void;
  onUpdateTimeDimension: (index: number, updates: Partial<TimeDimension>) => void;
  onRemoveTimeDimension: (index: number) => void;

  limit?: number;
  onLimitChange?: (limit: number) => void;

  // Actions
  onExecuteQuery: () => void;
  availableDimensions: Field[];
  availableMeasures: Field[];
  setGeneratedSql: (sql: string) => void;
  setSqlError: (error: string | null) => void;
  canExecuteQuery: boolean;
  resultFile?: string;
  isPreagg: boolean;
  executionTime: number | null;
};

export const SemanticExplorerContext = createContext<SemanticExplorerContextType | null>(null);
export type { SemanticExplorerContextType };

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
    isPreagg,
    setIsPreagg,
    selectedDimensions,
    setSelectedDimensions,
    selectedMeasures,
    setSelectedMeasures,
    filters,
    orders,
    variables,
    timeDimensions,
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
    addTimeDimension,
    updateTimeDimension,
    removeTimeDimension,
    toggleDimension,
    toggleMeasure,
    executionTime,
    setExecutionTime
  } = useSemanticQueryState();

  const { mutate: executeSemanticQuery, isPending: isExecuting } = useExecuteSemanticQuery();
  const { mutate: compileSemanticQuery, isPending: isCompiling } = useCompileSemanticQuery();

  const loading = isExecuting || isCompiling || dataLoading;
  const sqlLoading = isCompiling;
  const executeLoading = isExecuting;

  // Auto-compile query when selection changes
  useEffect(() => {
    if (
      !canExecuteQuery ||
      (selectedDimensions.length === 0 &&
        selectedMeasures.length === 0 &&
        timeDimensions.length === 0)
    )
      return;

    const request = buildSemanticQuery({
      topic,
      dimensions: selectedDimensions,
      measures: selectedMeasures,
      filters,
      orders,
      variables,
      timeDimensions
    });

    setGeneratedSql("");
    setSqlError(null);

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
    timeDimensions,
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
      variables,
      timeDimensions
    });

    const startTime = Date.now();
    setExecutionTime(null);

    executeSemanticQuery(request, {
      onSuccess: (data) => {
        if (data && typeof data === "object" && "file_name" in data) {
          setExecutionTime(data.execution_time_ms);
        } else {
          setExecutionTime(Date.now() - startTime);
        }
        if (data && typeof data === "object" && "file_name" in data) {
          setResultFile(data.file_name);
          setIsPreagg(data.is_preagg ?? false);
        } else {
          setIsPreagg(false);
        }
        setResult([]);
        setExecutionError(null);
      },
      onError: (error) => {
        setExecutionTime(Date.now() - startTime);
        setResult([]);
        setResultFile(undefined);
        setIsPreagg(false);
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
    timeDimensions,
    executeSemanticQuery,
    setResult,
    setResultFile,
    setIsPreagg,
    setExecutionError,
    setExecutionTime
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
      sqlLoading,
      executeLoading,
      refetchData,
      selectedDimensions,
      setSelectedDimensions,
      selectedMeasures,
      setSelectedMeasures,
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
      timeDimensions,
      onAddFilter: addFilter,
      onUpdateFilter: updateFilter,
      onRemoveFilter: removeFilter,
      onAddOrder: addOrder,
      onUpdateOrder: updateOrder,
      onRemoveOrder: removeOrder,
      onAddVariable: addVariable,
      onUpdateVariable: updateVariable,
      onRemoveVariable: removeVariable,
      onAddTimeDimension: addTimeDimension,
      onUpdateTimeDimension: updateTimeDimension,
      onRemoveTimeDimension: removeTimeDimension,
      onExecuteQuery: handleExecuteQuery,
      availableDimensions,
      setSqlError,
      setGeneratedSql,
      canExecuteQuery,
      availableMeasures,
      setResult,
      resultFile,
      setResultFile,
      isPreagg,
      executionTime
    }),
    [
      dataLoading,
      loadingError,
      loading,
      sqlLoading,
      executeLoading,
      refetchData,
      selectedDimensions,
      setSelectedDimensions,
      selectedMeasures,
      setSelectedMeasures,
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
      timeDimensions,
      addFilter,
      isPreagg,
      updateFilter,
      removeFilter,
      addOrder,
      updateOrder,
      removeOrder,
      addVariable,
      updateVariable,
      removeVariable,
      addTimeDimension,
      updateTimeDimension,
      removeTimeDimension,
      handleExecuteQuery,
      availableDimensions,
      setSqlError,
      setGeneratedSql,
      canExecuteQuery,
      availableMeasures,
      setResult,
      resultFile,
      setResultFile,
      executionTime
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
