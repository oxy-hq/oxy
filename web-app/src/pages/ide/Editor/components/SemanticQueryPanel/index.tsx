import { useEffect } from "react";
import { Tabs, TabsContent } from "@/components/ui/shadcn/tabs";
import { SemanticQueryFilter } from "@/services/api/semantic";
import TabsHeader from "./components/TabsHeader";
import FiltersSection from "./components/FiltersSection";
import VariablesSection from "./components/VariablesSection";
import SqlView from "./components/SqlView";
import ResultsView from "./components/ResultsView";

export interface Variable {
  key: string;
  value: string;
}

interface SemanticQueryPanelProps {
  result: string[][];
  showSql: boolean;
  setShowSql: (show: boolean) => void;
  generatedSql: string;
  sqlError: string | null;
  executionError: string | null;
  filters: SemanticQueryFilter[];
  variables: Variable[];
  onAddFilter: () => void;
  onUpdateFilter: (index: number, updates: SemanticQueryFilter) => void;
  onRemoveFilter: (index: number) => void;
  onAddVariable: () => void;
  onUpdateVariable: (index: number, updates: Partial<Variable>) => void;
  onRemoveVariable: (index: number) => void;
  onExecuteQuery: () => void;
  loading: boolean;
  canExecuteQuery: boolean;
  disabledMessage?: string;
  availableDimensions: { label: string; value: string }[];
  hasData: boolean;
}

const SemanticQueryPanel = ({
  result,
  showSql,
  setShowSql,
  generatedSql,
  sqlError,
  executionError,
  filters,
  variables,
  onAddFilter,
  onUpdateFilter,
  onRemoveFilter,
  onAddVariable,
  onUpdateVariable,
  onRemoveVariable,
  onExecuteQuery,
  loading,
  canExecuteQuery,
  disabledMessage,
  availableDimensions,
  hasData,
}: SemanticQueryPanelProps) => {
  useEffect(() => {
    if (generatedSql || sqlError) {
      setShowSql(true);
    }
  }, [generatedSql, setShowSql, sqlError]);

  useEffect(() => {
    if (result || executionError) {
      setShowSql(false);
    }
  }, [result, setShowSql, executionError]);

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
        onAddVariable={onAddVariable}
        onExecuteQuery={onExecuteQuery}
        loading={loading}
        canExecuteQuery={canExecuteQuery}
        disabledMessage={disabledMessage}
      />

      {hasData && (
        <FiltersSection
          filters={filters}
          availableDimensions={availableDimensions}
          onUpdateFilter={onUpdateFilter}
          onRemoveFilter={onRemoveFilter}
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
          <ResultsView result={result} executionError={executionError} />
        </TabsContent>
      </div>
    </Tabs>
  );
};

export default SemanticQueryPanel;
