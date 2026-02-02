import { AlertCircle, Loader2 } from "lucide-react";
import useDatabaseClient from "@/stores/useDatabaseClient";
import SqlResultsTable from "@/components/sql/SqlResultsTable";

export default function QueryResults() {
  const { tabs, activeTabId } = useDatabaseClient();
  const activeTab = tabs.find((t) => t.id === activeTabId);

  if (!activeTab) {
    return (
      <div className="h-full flex items-center justify-center text-muted-foreground">
        <p className="text-sm">Select a query to see results</p>
      </div>
    );
  }

  if (activeTab.isExecuting) {
    return (
      <div className="h-full flex flex-col items-center justify-center text-muted-foreground">
        <Loader2 className="h-8 w-8 animate-spin mb-2" />
        <p className="text-sm">Executing query...</p>
      </div>
    );
  }

  if (activeTab.error) {
    return (
      <div className="h-full flex flex-col items-center justify-center text-destructive">
        <AlertCircle className="h-8 w-8 mb-2" />
        <p className="text-sm text-center max-w-lg">{activeTab.error}</p>
      </div>
    );
  }

  if (!activeTab.results) {
    return (
      <div className="h-full flex items-center justify-center text-muted-foreground">
        <p className="text-sm">No results to display</p>
      </div>
    );
  }

  const { result, resultFile } = activeTab.results;

  return (
    <div
      className="flex-1 flex flex-col overflow-hidden min-h-0"
      style={{ width: "100%", height: "100%" }}
    >
      <div className="flex-1 flex flex-col overflow-hidden">
        <div className="flex-1 overflow-hidden">
          <SqlResultsTable result={result} resultFile={resultFile} />
        </div>
      </div>
    </div>
  );
}
