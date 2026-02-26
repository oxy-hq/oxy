import { useCallback, useMemo, useState } from "react";
import SqlResultsTable from "@/components/sql/SqlResultsTable";
import useTopicFieldOptions from "@/hooks/api/useTopicFieldOptions";
import type { SemanticQueryArtifact } from "@/types/artifact";
import ErrorsDisplay from "./ErrorsDisplay";
import type { FieldItem } from "./FieldList";
import FieldList from "./FieldList";
import FiltersDisplay from "./FiltersDisplay";
import LimitOffset from "./LimitOffset";
import OrdersDisplay from "./OrdersDisplay";
import SqlDisplay from "./SqlDisplay";
import TopicHeader from "./TopicHeader";

// Re-export sub-components for external use
export { default as CollapsibleSection } from "./CollapsibleSection";
export { default as ErrorsDisplay, ErrorBlock } from "./ErrorsDisplay";
export type { FieldItem } from "./FieldList";
export { default as FieldList } from "./FieldList";
export { default as FiltersDisplay } from "./FiltersDisplay";
export { default as LimitOffset } from "./LimitOffset";
export { default as OrdersDisplay } from "./OrdersDisplay";
export { default as SqlDisplay } from "./SqlDisplay";
export { default as TopicHeader } from "./TopicHeader";

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
    filters,
    orders,
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

  const { dimensions: availableDimensions, measures: availableMeasures } = useTopicFieldOptions(
    editable ? topic || undefined : undefined
  );

  const isModified = useMemo(
    () =>
      editable &&
      (dimensions.length !== originalDimensions.length ||
        measures.length !== originalMeasures.length ||
        dimensions.some((d, i) => d !== originalDimensions[i]) ||
        measures.some((m, i) => m !== originalMeasures[i])),
    [editable, dimensions, measures, originalDimensions, originalMeasures]
  );

  const handleRerun = useCallback(() => {
    const prompt =
      `Re-run the analysis using a modified semantic query for topic "${topic}". ` +
      `Use dimensions: [${dimensions.join(", ")}] and measures: [${measures.join(", ")}]. ` +
      `Keep the same filters and other parameters.`;
    onRerun?.(prompt);
  }, [topic, dimensions, measures, onRerun]);

  const dimensionItems: FieldItem[] = useMemo(
    () => availableDimensions.map((d) => ({ value: d.value, label: d.label })),
    [availableDimensions]
  );

  const measureItems: FieldItem[] = useMemo(
    () => availableMeasures.map((m) => ({ value: m.value, label: m.label })),
    [availableMeasures]
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

      <FiltersDisplay filters={filters} />
      <OrdersDisplay orders={orders} />
      <LimitOffset limit={limit} offset={offset} />

      {sql_query && <SqlDisplay sql={sql_query} defaultOpen={sqlDefaultOpen} />}

      <ErrorsDisplay
        validationError={validation_error}
        sqlGenerationError={sql_generation_error}
        executionError={error}
      />

      {(result || result_file) && (
        <div className='min-h-[200px] flex-1'>
          <SqlResultsTable result={result} resultFile={result_file} />
        </div>
      )}
    </div>
  );
};

export default SemanticQueryPanel;
