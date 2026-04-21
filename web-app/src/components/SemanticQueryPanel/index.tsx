import { useCallback, useMemo, useState } from "react";
import SqlResultsTable from "@/components/sql/SqlResultsTable";
import ErrorAlert from "@/components/ui/ErrorAlert";
import useTopicFieldOptions from "@/hooks/api/useTopicFieldOptions";
import type { SemanticQueryFilter } from "@/services/api/semantic";
import type { SemanticQueryArtifact } from "@/types/artifact";
import type { FieldItem } from "./FieldList";
import FieldList from "./FieldList";
import FiltersDisplay from "./FiltersDisplay";
import LimitOffset from "./LimitOffset";
import OrdersDisplay from "./OrdersDisplay";
import SqlDisplay from "./SqlDisplay";
import TimeDimensionsDisplay from "./TimeDimensionsDisplay";
import TopicHeader from "./TopicHeader";

interface SemanticQueryPanelProps {
  artifact: SemanticQueryArtifact;
  /** Enable editing mode with dimension/measure selection */
  editable?: boolean;
  /** Callback when user wants to re-run with modified parameters */
  onRerun?: (prompt: string) => void;
  /** Show the SQL section expanded by default */
  sqlDefaultOpen?: boolean;
  /** Show database info in header */
  showDatabase?: boolean;
}

const SemanticQueryPanel = ({
  artifact,
  editable = false,
  onRerun,
  sqlDefaultOpen = false,
  showDatabase = false
}: SemanticQueryPanelProps) => {
  const {
    topic,
    database,
    dimensions: originalDimensions,
    measures: originalMeasures,
    time_dimensions: timeDimensions,
    filters: originalFilters,
    orders: originalOrders,
    limit,
    offset,
    sql_query,
    validation_error,
    sql_generation_error,
    error,
    result,
    result_file
  } = artifact.content.value;

  const [dimensions, setDimensions] = useState<string[]>(originalDimensions);
  const [measures, setMeasures] = useState<string[]>(originalMeasures);
  const [filters, setFilters] = useState<SemanticQueryFilter[]>(
    originalFilters as SemanticQueryFilter[]
  );
  const [orders, setOrders] = useState<{ field: string; direction: "asc" | "desc" }[]>(
    originalOrders as { field: string; direction: "asc" | "desc" }[]
  );

  const {
    dimensions: availableDimensions,
    measures: availableMeasures,
    allFields
  } = useTopicFieldOptions(editable ? topic || undefined : undefined);

  const isModified = useMemo(() => {
    if (!editable) return false;
    const dimsChanged =
      dimensions.length !== originalDimensions.length ||
      dimensions.some((d, i) => d !== originalDimensions[i]);
    const measChanged =
      measures.length !== originalMeasures.length ||
      measures.some((m, i) => m !== originalMeasures[i]);
    const filtersChanged = JSON.stringify(filters) !== JSON.stringify(originalFilters);
    const ordersChanged = JSON.stringify(orders) !== JSON.stringify(originalOrders);
    return dimsChanged || measChanged || filtersChanged || ordersChanged;
  }, [
    editable,
    dimensions,
    measures,
    filters,
    orders,
    originalDimensions,
    originalMeasures,
    originalFilters,
    originalOrders
  ]);

  const handleRerun = useCallback(() => {
    const parts = [`Re-run the analysis using a modified semantic query for topic "${topic}".`];
    parts.push(
      `Use dimensions: [${dimensions.join(", ")}] and measures: [${measures.join(", ")}].`
    );
    if (filters.length > 0) {
      parts.push(`Use filters: ${JSON.stringify(filters)}.`);
    } else {
      parts.push("No filters.");
    }
    if (orders.length > 0) {
      parts.push(`Use ordering: ${JSON.stringify(orders)}.`);
    }
    onRerun?.(parts.join(" "));
  }, [topic, dimensions, measures, filters, orders, onRerun]);

  const dimensionItems: FieldItem[] = useMemo(
    () => availableDimensions.map((d) => ({ value: d.value, label: d.label })),
    [availableDimensions]
  );

  const measureItems: FieldItem[] = useMemo(
    () => availableMeasures.map((m) => ({ value: m.value, label: m.label })),
    [availableMeasures]
  );

  const filterDimensions = useMemo(
    () =>
      availableDimensions.map((d) => ({
        name: d.label,
        fullName: d.value,
        type: d.dataType
      })),
    [availableDimensions]
  );

  const sortFields = useMemo(
    () => allFields.map((f) => ({ name: f.label, fullName: f.value })),
    [allFields]
  );

  const handleDimensionChange = useCallback(
    (index: number, value: string) =>
      setDimensions((prev) => prev.map((d, i) => (i === index ? value : d))),
    []
  );

  const handleDimensionRemove = useCallback(
    (index: number) => setDimensions((prev) => prev.filter((_, i) => i !== index)),
    []
  );

  const handleDimensionAdd = useCallback(() => {
    const unused = availableDimensions.find((d) => !dimensions.includes(d.value));
    if (unused) setDimensions((prev) => [...prev, unused.value]);
  }, [availableDimensions, dimensions]);

  const handleMeasureChange = useCallback(
    (index: number, value: string) =>
      setMeasures((prev) => prev.map((m, i) => (i === index ? value : m))),
    []
  );

  const handleMeasureRemove = useCallback(
    (index: number) => setMeasures((prev) => prev.filter((_, i) => i !== index)),
    []
  );

  const handleMeasureAdd = useCallback(() => {
    const unused = availableMeasures.find((m) => !measures.includes(m.value));
    if (unused) setMeasures((prev) => [...prev, unused.value]);
  }, [availableMeasures, measures]);

  // Filter handlers
  const handleAddFilter = useCallback(() => {
    const firstDim = availableDimensions[0];
    if (firstDim) {
      setFilters((prev) => [...prev, { field: firstDim.value, op: "eq", value: "" }]);
    }
  }, [availableDimensions]);

  const handleUpdateFilter = useCallback((index: number, updates: SemanticQueryFilter) => {
    setFilters((prev) => prev.map((f, i) => (i === index ? updates : f)));
  }, []);

  const handleRemoveFilter = useCallback((index: number) => {
    setFilters((prev) => prev.filter((_, i) => i !== index));
  }, []);

  // Order handlers
  const handleAddOrder = useCallback(() => {
    const selectedFields = [...dimensions, ...measures];
    const firstField = selectedFields[0];
    if (firstField) {
      setOrders((prev) => [...prev, { field: firstField, direction: "asc" }]);
    }
  }, [dimensions, measures]);

  const handleUpdateOrder = useCallback(
    (index: number, updates: { field: string; direction: "asc" | "desc" }) => {
      setOrders((prev) => prev.map((o, i) => (i === index ? updates : o)));
    },
    []
  );

  const handleRemoveOrder = useCallback((index: number) => {
    setOrders((prev) => prev.filter((_, i) => i !== index));
  }, []);

  return (
    <div className='flex h-full flex-col overflow-y-auto'>
      <TopicHeader
        topic={topic}
        database={showDatabase ? database : undefined}
        isModified={isModified}
        onRerun={handleRerun}
      />

      <FieldList
        title='Dimensions'
        fields={editable ? dimensions : originalDimensions}
        availableItems={dimensionItems}
        placeholder='Select dimension...'
        searchPlaceholder='Search dimensions...'
        addLabel='Add dimension'
        editable={editable}
        onFieldChange={handleDimensionChange}
        onFieldRemove={handleDimensionRemove}
        onFieldAdd={handleDimensionAdd}
      />

      <FieldList
        title='Measures'
        fields={editable ? measures : originalMeasures}
        availableItems={measureItems}
        placeholder='Select measure...'
        searchPlaceholder='Search measures...'
        addLabel='Add measure'
        editable={editable}
        onFieldChange={handleMeasureChange}
        onFieldRemove={handleMeasureRemove}
        onFieldAdd={handleMeasureAdd}
      />

      <TimeDimensionsDisplay timeDimensions={timeDimensions ?? []} />

      <FiltersDisplay
        filters={editable ? filters : (originalFilters as SemanticQueryFilter[])}
        editable={editable}
        availableDimensions={filterDimensions}
        onAddFilter={handleAddFilter}
        onUpdateFilter={handleUpdateFilter}
        onRemoveFilter={handleRemoveFilter}
      />

      <OrdersDisplay
        orders={editable ? orders : originalOrders}
        editable={editable}
        availableFields={sortFields}
        onAddOrder={handleAddOrder}
        onUpdateOrder={handleUpdateOrder}
        onRemoveOrder={handleRemoveOrder}
      />

      <LimitOffset limit={limit} offset={offset} />

      {sql_query && <SqlDisplay sql={sql_query} defaultOpen={sqlDefaultOpen} />}

      <div className='flex flex-col gap-2'>
        {validation_error && <ErrorAlert title='Validation Error' message={validation_error} />}
        {sql_generation_error && (
          <ErrorAlert title='SQL Generation Error' message={sql_generation_error} />
        )}
        {error && <ErrorAlert title='Execution Error' message={error} />}
      </div>

      {(result || result_file) && (
        <div className='min-h-[200px] flex-1'>
          <SqlResultsTable result={result} resultFile={result_file} />
        </div>
      )}
    </div>
  );
};

export default SemanticQueryPanel;
