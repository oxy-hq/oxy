import { Download, Plus, X } from "lucide-react";
import { useEffect } from "react";
import { Button } from "@/components/ui/shadcn/button";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/shadcn/tooltip";
import Papa from "papaparse";
import { handleDownloadFile } from "@/libs/utils/string";
import { Prism as SyntaxHighlighter } from "react-syntax-highlighter";
import { oneDark } from "react-syntax-highlighter/dist/esm/styles/prism";
import HeaderActions from "./HeaderActions";
import Results from "../../Sql/Results";
import {
  Tabs,
  TabsContent,
  TabsList,
  TabsTrigger,
} from "@/components/ui/shadcn/tabs";

export interface Filter {
  field: string;
  operator: string;
  value: string;
}

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
  filters: Filter[];
  variables: Variable[];
  onAddFilter: () => void;
  onUpdateFilter: (index: number, updates: Partial<Filter>) => void;
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
      className="flex-1 flex flex-col overflow-hidden"
    >
      {/* Top bar with Results/SQL tabs and action buttons */}
      <div className="flex items-center justify-between px-4 py-2 border-b">
        <TabsList>
          <TabsTrigger value="results">Results</TabsTrigger>
          <TabsTrigger value="sql">SQL</TabsTrigger>
        </TabsList>
        <div className="flex items-center gap-2">
          {!showSql && result.length > 0 && (
            <Tooltip>
              <TooltipTrigger asChild>
                <Button
                  size="sm"
                  variant="ghost"
                  onClick={() => {
                    const csvContent = Papa.unparse(result, {
                      delimiter: ",",
                      header: true,
                      skipEmptyLines: true,
                    });
                    const blob = new Blob([csvContent], {
                      type: "text/csv;charset=utf-8;",
                    });
                    handleDownloadFile(blob, "query_results.csv");
                  }}
                  className="h-7 w-7 p-0"
                >
                  <Download className="w-4 h-4" />
                </Button>
              </TooltipTrigger>
              <TooltipContent>Download results as CSV</TooltipContent>
            </Tooltip>
          )}
          {hasData && (
            <>
              <Button
                size="sm"
                variant="outline"
                onClick={onAddFilter}
                className="h-7"
              >
                <Plus className="w-3 h-3 mr-1" />
                Add Filter
              </Button>
              <Button
                size="sm"
                variant="outline"
                onClick={onAddVariable}
                className="h-7"
              >
                <Plus className="w-3 h-3 mr-1" />
                Add Variable
              </Button>
              <HeaderActions
                onExecuteQuery={onExecuteQuery}
                loading={loading}
                disabled={!canExecuteQuery}
                disabledMessage={disabledMessage}
              />
            </>
          )}
        </div>
      </div>

      {/* Filters Section */}
      {hasData && filters.length > 0 && (
        <div className="border-b p-3 space-y-2">
          {filters.map((filter, index) => (
            <div key={index} className="flex items-center gap-2">
              <select
                value={filter.field}
                onChange={(e) =>
                  onUpdateFilter(index, { field: e.target.value })
                }
                className="text-xs border rounded px-2 py-1 bg-background"
              >
                {availableDimensions.map((dim) => (
                  <option key={dim.value} value={dim.value}>
                    {dim.label}
                  </option>
                ))}
              </select>
              <select
                value={filter.operator}
                onChange={(e) =>
                  onUpdateFilter(index, { operator: e.target.value })
                }
                className="text-xs border rounded px-2 py-1 bg-background"
              >
                <option value="=">=</option>
                <option value="!=">!=</option>
                <option value=">">{">"}</option>
                <option value=">=">{">="}</option>
                <option value="<">{"<"}</option>
                <option value="<=">{"<="}</option>
                <option value="LIKE">LIKE</option>
                <option value="NOT LIKE">NOT LIKE</option>
                <option value="IN">IN</option>
                <option value="IS NULL">IS NULL</option>
                <option value="IS NOT NULL">IS NOT NULL</option>
              </select>
              {filter.operator !== "IS NULL" &&
                filter.operator !== "IS NOT NULL" && (
                  <input
                    type="text"
                    value={filter.value}
                    onChange={(e) =>
                      onUpdateFilter(index, { value: e.target.value })
                    }
                    placeholder="Value"
                    className="flex-1 text-xs border rounded px-2 py-1 bg-background"
                  />
                )}
              <Button
                size="sm"
                variant="ghost"
                onClick={() => onRemoveFilter(index)}
                className="h-7 w-7 p-0"
              >
                <X className="w-3 h-3" />
              </Button>
            </div>
          ))}
        </div>
      )}

      {/* Variables Section */}
      {variables.length > 0 && (
        <div className="border-b p-3 space-y-2">
          {variables.map((variable, index) => (
            <div key={index} className="flex items-center gap-2">
              <input
                type="text"
                value={variable.key}
                onChange={(e) =>
                  onUpdateVariable(index, { key: e.target.value })
                }
                placeholder="Variable Name"
                className="text-xs border rounded px-2 py-1 bg-background"
              />
              <span className="text-xs text-muted-foreground">=</span>
              <input
                type="text"
                value={variable.value}
                onChange={(e) =>
                  onUpdateVariable(index, { value: e.target.value })
                }
                placeholder="Value"
                className="flex-1 text-xs border rounded px-2 py-1 bg-background"
              />
              <Button
                size="sm"
                variant="ghost"
                onClick={() => onRemoveVariable(index)}
                className="h-7 w-7 p-0"
              >
                <X className="w-3 h-3" />
              </Button>
            </div>
          ))}
        </div>
      )}

      {/* Content */}
      <div className="flex-1 overflow-hidden">
        <TabsContent value="sql" className="h-full mt-0">
          <div className="h-full overflow-auto customScrollbar p-4">
            {(() => {
              if (sqlError) {
                return (
                  <div className="text-xs font-mono bg-destructive/10 text-destructive p-4 rounded whitespace-pre-wrap">
                    {sqlError}
                  </div>
                );
              }
              if (generatedSql) {
                return (
                  <SyntaxHighlighter
                    language="sql"
                    style={oneDark}
                    customStyle={{ margin: 0, borderRadius: "0.5rem" }}
                    className="text-xs font-mono"
                  >
                    {generatedSql}
                  </SyntaxHighlighter>
                );
              }
              return (
                <div className="flex items-center justify-center h-full text-sm text-muted-foreground">
                  Run a query to see the generated SQL
                </div>
              );
            })()}
          </div>
        </TabsContent>
        <TabsContent value="results" className="h-full mt-0">
          {executionError ? (
            <div className="h-full overflow-auto customScrollbar p-4">
              <div className="text-xs font-mono bg-destructive/10 text-destructive p-4 rounded whitespace-pre-wrap">
                {executionError}
              </div>
            </div>
          ) : (
            <Results result={result} />
          )}
        </TabsContent>
      </div>
    </Tabs>
  );
};

export default SemanticQueryPanel;
