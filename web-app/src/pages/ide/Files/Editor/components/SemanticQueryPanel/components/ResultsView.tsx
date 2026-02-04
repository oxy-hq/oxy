import SqlResultsTable from "@/components/sql/SqlResultsTable";

interface ResultsViewProps {
  result?: string[][];
  resultFile?: string;
  executionError: string | null;
}

const ResultsView = ({ result, resultFile, executionError }: ResultsViewProps) => {
  if (executionError) {
    return (
      <div className='customScrollbar h-full overflow-auto p-4'>
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
