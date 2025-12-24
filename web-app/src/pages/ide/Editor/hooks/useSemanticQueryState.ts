import { useState } from "react";
import { SemanticQueryFilter } from "@/services/api/semantic";
import { Variable } from "../components/SemanticQueryPanel";

export const useSemanticQueryState = () => {
  const [result, setResult] = useState<string[][]>([]);
  const [selectedDimensions, setSelectedDimensions] = useState<string[]>([]);
  const [selectedMeasures, setSelectedMeasures] = useState<string[]>([]);
  const [filters, setFilters] = useState<SemanticQueryFilter[]>([]);
  const [variables, setVariables] = useState<Variable[]>([]);
  const [showSql, setShowSql] = useState(false);
  const [generatedSql, setGeneratedSql] = useState("");
  const [sqlError, setSqlError] = useState<string | null>(null);
  const [executionError, setExecutionError] = useState<string | null>(null);

  const addFilter = (initialField: string) => {
    setFilters([
      ...filters,
      {
        field: initialField,
        op: "eq",
        value: "",
      } as SemanticQueryFilter,
    ]);
  };

  const updateFilter = (index: number, updates: SemanticQueryFilter) => {
    const newFilters = [...filters];
    newFilters[index] = updates;
    setFilters(newFilters);
  };

  const removeFilter = (index: number) => {
    setFilters(filters.filter((_, i) => i !== index));
  };

  const addVariable = () => {
    setVariables([...variables, { key: "", value: "" }]);
  };

  const updateVariable = (index: number, updates: Partial<Variable>) => {
    const newVariables = [...variables];
    newVariables[index] = { ...newVariables[index], ...updates };
    setVariables(newVariables);
  };

  const removeVariable = (index: number) => {
    setVariables(variables.filter((_, i) => i !== index));
  };

  const toggleDimension = (fullName: string) => {
    setSelectedDimensions((prev) =>
      prev.includes(fullName)
        ? prev.filter((d) => d !== fullName)
        : [...prev, fullName],
    );
  };

  const toggleMeasure = (fullName: string) => {
    setSelectedMeasures((prev) =>
      prev.includes(fullName)
        ? prev.filter((m) => m !== fullName)
        : [...prev, fullName],
    );
  };

  return {
    result,
    setResult,
    selectedDimensions,
    setSelectedDimensions,
    selectedMeasures,
    setSelectedMeasures,
    filters,
    setFilters,
    variables,
    setVariables,
    showSql,
    setShowSql,
    generatedSql,
    setGeneratedSql,
    sqlError,
    setSqlError,
    executionError,
    setExecutionError,
    addFilter,
    updateFilter,
    removeFilter,
    addVariable,
    updateVariable,
    removeVariable,
    toggleDimension,
    toggleMeasure,
  };
};
