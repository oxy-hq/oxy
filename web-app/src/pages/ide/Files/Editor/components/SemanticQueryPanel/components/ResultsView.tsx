import SqlResultsTable from "@/components/sql/SqlResultsTable";

interface ResultsViewProps {
  result?: string[][];
  resultFile?: string;
  executionError: string | null;
}

const ResultsView = ({
  result,
  resultFile,
  executionError,
}: ResultsViewProps) => {
  if (executionError) {
    return (
      <div className="h-full overflow-auto customScrollbar p-4">
        <div className="text-xs font-mono bg-destructive/10 text-destructive p-4 rounded whitespace-pre-wrap">
          {executionError}
        </div>
      </div>
    );
  }

  return (
    <div className="h-full min-h-0">
      <SqlResultsTable result={result} resultFile={resultFile} />
    </div>
  );
};

export default ResultsView;
