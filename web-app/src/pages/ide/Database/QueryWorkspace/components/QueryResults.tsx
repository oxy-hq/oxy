import { AlertCircle, Loader2 } from "lucide-react";
import SqlResultsTable from "@/components/sql/SqlResultsTable";
import useDatabaseClient from "@/stores/useDatabaseClient";

export default function QueryResults() {
  const { tabs, activeTabId } = useDatabaseClient();
  const activeTab = tabs.find((t) => t.id === activeTabId);

  if (!activeTab) {
    return (
      <div className='flex h-full items-center justify-center text-muted-foreground'>
        <p className='text-sm'>Select a query to see results</p>
      </div>
    );
  }

  if (activeTab.isExecuting) {
    return (
      <div className='flex h-full flex-col items-center justify-center text-muted-foreground'>
        <Loader2 className='mb-2 h-8 w-8 animate-spin' />
        <p className='text-sm'>Executing query...</p>
      </div>
    );
  }

  if (activeTab.error) {
    return (
      <div className='flex h-full flex-col items-center justify-center text-destructive'>
        <AlertCircle className='mb-2 h-8 w-8' />
        <p className='max-w-lg text-center text-sm'>{activeTab.error}</p>
      </div>
    );
  }

  if (!activeTab.results) {
    return (
      <div className='flex h-full items-center justify-center text-muted-foreground'>
        <p className='text-sm'>No results to display</p>
      </div>
    );
  }

  const { result, resultFile } = activeTab.results;

  return (
    <div
      className='flex min-h-0 flex-1 flex-col overflow-hidden'
      style={{ width: "100%", height: "100%" }}
    >
      <div className='flex flex-1 flex-col overflow-hidden'>
        <div className='flex-1 overflow-hidden'>
          <SqlResultsTable result={result} resultFile={resultFile} />
        </div>
      </div>
    </div>
  );
}
