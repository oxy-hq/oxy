import { useEffect, useMemo } from "react";
import { Tabs, TabsContent } from "@/components/ui/shadcn/tabs";
import { useSemanticExplorerContext } from "../../contexts/SemanticExplorerContext";
import FiltersSection from "./components/FiltersSection";
import ResultsView from "./components/ResultsView";
import SortsSection from "./components/SortsSection";
import SqlView from "./components/SqlView";
import TabsHeader from "./components/TabsHeader";
import VariablesSection from "./components/VariablesSection";

export interface Variable {
  key: string;
  value: string;
}

const SemanticQueryPanel = () => {
  const {
    result,
    showSql,
    setShowSql,
    generatedSql,
    sqlError,
    executionError,
    filters,
    orders,
    variables,
    onAddFilter,
    onUpdateFilter,
    onRemoveFilter,
    onAddOrder,
    onUpdateOrder,
    onRemoveOrder,
    onAddVariable,
    onUpdateVariable,
    onRemoveVariable,
    resultFile,
    onExecuteQuery,
    loading,
    availableDimensions,
    availableMeasures,
    selectedDimensions,
    selectedMeasures
  } = useSemanticExplorerContext();
  const availableFields = useMemo(() => {
    return [...availableDimensions, ...availableMeasures];
  }, [availableDimensions, availableMeasures]);

  const selectedFields = useMemo(() => {
    const selectedDimensionFields = selectedDimensions
      .map((dim) => availableFields.find((f) => f.fullName === dim))
      .filter((field): field is (typeof availableFields)[number] => field !== undefined);

    const selectedMeasureFields = selectedMeasures
      .map((mes) => availableFields.find((f) => f.fullName === mes))
      .filter((field): field is (typeof availableFields)[number] => field !== undefined);

    return [...selectedDimensionFields, ...selectedMeasureFields];
  }, [selectedDimensions, selectedMeasures, availableFields]);

  const canExecuteQuery = selectedDimensions.length > 0 || selectedMeasures.length > 0;

  useEffect(() => {
    if (generatedSql || sqlError) {
      setShowSql(true);
    }
  }, [generatedSql, setShowSql, sqlError]);

  useEffect(() => {
    if (result.length > 0 || resultFile || executionError) {
      setShowSql(false);
    }
  }, [result, resultFile, setShowSql, executionError]);

  return (
    <Tabs
      value={showSql ? "sql" : "results"}
      onValueChange={(value) => setShowSql(value === "sql")}
      className='flex min-h-0 flex-1 flex-col overflow-hidden'
    >
      <TabsHeader
        showSql={showSql}
        hasResults={result.length > 0}
        result={result}
        onAddFilter={onAddFilter}
        onAddOrder={onAddOrder}
        onAddVariable={onAddVariable}
        onExecuteQuery={onExecuteQuery}
        loading={loading}
        canExecuteQuery={canExecuteQuery}
        disabledMessage=''
        hasSelectedFields={selectedFields.length > 0}
      />

      <FiltersSection
        filters={filters}
        availableDimensions={availableDimensions}
        onUpdateFilter={onUpdateFilter}
        onRemoveFilter={onRemoveFilter}
      />

      <SortsSection
        orders={orders}
        availableFields={selectedFields}
        onUpdateOrder={onUpdateOrder}
        onRemoveOrder={onRemoveOrder}
      />

      <VariablesSection
        variables={variables}
        onUpdateVariable={onUpdateVariable}
        onRemoveVariable={onRemoveVariable}
      />

      <div className='min-h-0 flex-1 overflow-hidden'>
        <TabsContent value='sql' className='mt-0 h-full'>
          <SqlView generatedSql={generatedSql} sqlError={sqlError} />
        </TabsContent>
        <TabsContent value='results' className='mt-0 h-full'>
          <ResultsView result={result} resultFile={resultFile} executionError={executionError} />
        </TabsContent>
      </div>
    </Tabs>
  );
};

export default SemanticQueryPanel;
