import type { ReactNode } from "react";
import SqlResultsTable from "@/components/sql/SqlResultsTable";
import ErrorAlert from "@/components/ui/ErrorAlert";
import { Spinner } from "@/components/ui/shadcn/spinner";

interface ResultsViewProps {
  result?: string[][];
  resultFile?: string;
  executionError: string | null;
  loading?: boolean;
  loadingIndicator?: ReactNode;
}

const ResultsView = ({
  result,
  resultFile,
  executionError,
  loading,
  loadingIndicator
}: ResultsViewProps) => {
  if (loading) {
    return (
      <div className='flex h-full flex-col items-center justify-center gap-2'>
        {loadingIndicator ?? <Spinner className='size-6 text-muted-foreground' />}
      </div>
    );
  }

  if (executionError) {
    return (
      <div className='h-full overflow-auto p-4'>
        <ErrorAlert>
          <div className='whitespace-pre-wrap font-mono text-xs'>{executionError}</div>
        </ErrorAlert>
      </div>
    );
  }

  return (
    <div className='h-full min-h-0'>
      <SqlResultsTable result={result} resultFile={resultFile} />
    </div>
  );
};

export default ResultsView;
