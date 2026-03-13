import { Loader2 } from "lucide-react";
import type { ReactNode } from "react";
import SqlResultsTable from "@/components/sql/SqlResultsTable";

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
        {loadingIndicator ?? (
          <>
            <Loader2 className='h-5 w-5 animate-spin text-muted-foreground' />
            <span className='text-muted-foreground text-sm'>Executing query...</span>
          </>
        )}
      </div>
    );
  }

  if (executionError) {
    return (
      <div className='customScrollbar h-full overflow-auto px-4 py-2'>
        <div className='whitespace-pre-wrap rounded bg-destructive/10 p-4 font-mono text-destructive text-xs'>
          {executionError}
        </div>
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
