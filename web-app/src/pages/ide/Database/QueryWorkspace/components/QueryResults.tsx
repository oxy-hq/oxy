import SqlResultsTable from "@/components/sql/SqlResultsTable";
import ErrorAlert from "@/components/ui/ErrorAlert";
import { Spinner } from "@/components/ui/shadcn/spinner";
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
        <Spinner className='mb-2 size-8' />
      </div>
    );
  }

  if (activeTab.error) {
    return (
      <div className='flex h-full flex-col items-center justify-center p-4'>
        <ErrorAlert message={activeTab.error} className='max-w-lg' />
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
