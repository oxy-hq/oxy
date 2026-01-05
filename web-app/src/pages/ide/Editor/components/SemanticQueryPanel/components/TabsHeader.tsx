import { Download, Plus } from "lucide-react";
import { Button } from "@/components/ui/shadcn/button";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/shadcn/tooltip";
import Papa from "papaparse";
import { handleDownloadFile } from "@/libs/utils/string";
import HeaderActions from "../HeaderActions";
import { TabsList, TabsTrigger } from "@/components/ui/shadcn/tabs";

interface TabsHeaderProps {
  showSql: boolean;
  hasResults: boolean;
  result: string[][];
  hasData: boolean;
  onAddFilter: () => void;
  onAddOrder: () => void;
  onAddVariable: () => void;
  onExecuteQuery: () => void;
  loading: boolean;
  canExecuteQuery: boolean;
  disabledMessage?: string;
  hasSelectedFields: boolean;
}

const TabsHeader = ({
  showSql,
  hasResults,
  result,
  hasData,
  onAddFilter,
  onAddOrder,
  onAddVariable,
  onExecuteQuery,
  loading,
  canExecuteQuery,
  disabledMessage,
  hasSelectedFields,
}: TabsHeaderProps) => {
  const handleDownloadCsv = () => {
    const csvContent = Papa.unparse(result, {
      delimiter: ",",
      header: true,
      skipEmptyLines: true,
    });
    const blob = new Blob([csvContent], {
      type: "text/csv;charset=utf-8;",
    });
    handleDownloadFile(blob, "query_results.csv");
  };

  return (
    <div className="flex items-center justify-between px-4 py-2 border-b">
      <TabsList>
        <TabsTrigger value="results">Results</TabsTrigger>
        <TabsTrigger value="sql">SQL</TabsTrigger>
      </TabsList>
      <div className="flex items-center gap-2">
        {!showSql && hasResults && (
          <Tooltip>
            <TooltipTrigger asChild>
              <Button
                size="sm"
                variant="ghost"
                onClick={handleDownloadCsv}
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
              onClick={onAddOrder}
              className="h-7"
              disabled={!hasSelectedFields}
            >
              <Plus className="w-3 h-3 mr-1" />
              Add Sort
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
  );
};

export default TabsHeader;
