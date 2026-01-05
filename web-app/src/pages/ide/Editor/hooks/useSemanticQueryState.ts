import { useState } from "react";
import {
  SemanticQueryFilter,
  SemanticQueryOrder,
} from "@/services/api/semantic";
import { Variable } from "../components/SemanticQueryPanel";

export const useSemanticQueryState = () => {
  const [result, setResult] = useState<string[][]>([]);
  const [selectedDimensions, setSelectedDimensions] = useState<string[]>([]);
  const [selectedMeasures, setSelectedMeasures] = useState<string[]>([]);
  const [filters, setFilters] = useState<SemanticQueryFilter[]>([]);
  const [orders, setOrders] = useState<SemanticQueryOrder[]>([]);
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

  const addOrder = (initialField: string) => {
    setOrders([
      ...orders,
      {
        field: initialField,
        direction: "asc",
      },
    ]);
  };

  const updateOrder = (index: number, updates: SemanticQueryOrder) => {
    const newOrders = [...orders];
    newOrders[index] = updates;
    setOrders(newOrders);
  };

  const removeOrder = (index: number) => {
    setOrders(orders.filter((_, i) => i !== index));
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

  const removeOrdersForField = (fieldName: string) => {
    setOrders((prevOrders) =>
      prevOrders.filter((order) => order.field !== fieldName),
    );
  };

  const toggleDimension = (fullName: string) => {
    const isRemoving = selectedDimensions.includes(fullName);
    if (isRemoving) {
      removeOrdersForField(fullName);
      setSelectedDimensions((prev) => prev.filter((d) => d !== fullName));
    } else {
      setSelectedDimensions((prev) => [...prev, fullName]);
    }
  };

  const toggleMeasure = (fullName: string) => {
    const isRemoving = selectedMeasures.includes(fullName);
    if (isRemoving) {
      removeOrdersForField(fullName);
      setSelectedMeasures((prev) => prev.filter((m) => m !== fullName));
    } else {
      setSelectedMeasures((prev) => [...prev, fullName]);
    }
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
    orders,
    setOrders,
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
    addOrder,
    updateOrder,
    removeOrder,
    addVariable,
    updateVariable,
    removeVariable,
    toggleDimension,
    toggleMeasure,
  };
};
