import { useEffect } from "react";
import { Tabs, TabsContent } from "@/components/ui/shadcn/tabs";
import {
  SemanticQueryFilter,
  SemanticQueryOrder,
} from "@/services/api/semantic";
import TabsHeader from "./components/TabsHeader";
import FiltersSection from "./components/FiltersSection";
import SortsSection from "./components/SortsSection";
import VariablesSection from "./components/VariablesSection";
import SqlView from "./components/SqlView";
import ResultsView from "./components/ResultsView";

export interface Variable {
  key: string;
  value: string;
}

interface SemanticQueryPanelProps {
  result: string[][];
  resultFile?: string;
  showSql: boolean;
  setShowSql: (show: boolean) => void;
  generatedSql: string;
  sqlError: string | null;
  executionError: string | null;
  filters: SemanticQueryFilter[];
  orders: SemanticQueryOrder[];
  variables: Variable[];
  onAddFilter: () => void;
  onUpdateFilter: (index: number, updates: SemanticQueryFilter) => void;
  onRemoveFilter: (index: number) => void;
  onAddOrder: () => void;
  onUpdateOrder: (index: number, updates: SemanticQueryOrder) => void;
  onRemoveOrder: (index: number) => void;
  onAddVariable: () => void;
  onUpdateVariable: (index: number, updates: Partial<Variable>) => void;
  onRemoveVariable: (index: number) => void;
  onExecuteQuery: () => void;
  loading: boolean;
  canExecuteQuery: boolean;
  disabledMessage?: string;
  availableDimensions: { label: string; value: string }[];
  selectedFields: { label: string; value: string }[];
  hasData: boolean;
}

const SemanticQueryPanel = ({
  result,
  resultFile,
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
  onExecuteQuery,
  loading,
  canExecuteQuery,
  disabledMessage,
  availableDimensions,
  selectedFields,
  hasData,
}: SemanticQueryPanelProps) => {
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
      className="flex-1 flex flex-col min-h-0 overflow-hidden"
    >
      <TabsHeader
        showSql={showSql}
        hasResults={result.length > 0}
        result={result}
        hasData={hasData}
        onAddFilter={onAddFilter}
        onAddOrder={onAddOrder}
        onAddVariable={onAddVariable}
        onExecuteQuery={onExecuteQuery}
        loading={loading}
        canExecuteQuery={canExecuteQuery}
        disabledMessage={disabledMessage}
        hasSelectedFields={selectedFields.length > 0}
      />

      {hasData && (
        <FiltersSection
          filters={filters}
          availableDimensions={availableDimensions}
          onUpdateFilter={onUpdateFilter}
          onRemoveFilter={onRemoveFilter}
        />
      )}

      {hasData && (
        <SortsSection
          orders={orders}
          availableFields={selectedFields}
          onUpdateOrder={onUpdateOrder}
          onRemoveOrder={onRemoveOrder}
        />
      )}

      <VariablesSection
        variables={variables}
        onUpdateVariable={onUpdateVariable}
        onRemoveVariable={onRemoveVariable}
      />

      <div className="flex-1 min-h-0 overflow-hidden">
        <TabsContent value="sql" className="h-full mt-0">
          <SqlView generatedSql={generatedSql} sqlError={sqlError} />
        </TabsContent>
        <TabsContent value="results" className="h-full mt-0">
          <ResultsView
            result={result}
            resultFile={resultFile}
            executionError={executionError}
          />
        </TabsContent>
      </div>
    </Tabs>
  );
};

export default SemanticQueryPanel;
