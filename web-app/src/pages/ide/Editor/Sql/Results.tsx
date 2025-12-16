import EmptyState from "@/components/ui/EmptyState";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/shadcn/table";

interface ResultsProps {
  result: string[][];
  hideDownload?: boolean;
}

const Results = ({ result }: ResultsProps) => {
  return (
    <div className="flex flex-col h-full overflow-hidden">
      <div className="flex-1 overflow-auto customScrollbar">
        {result.length > 0 ? (
          <Table className="font-mono text-xs">
            <TableHeader>
              <TableRow className="border-b hover:bg-transparent">
                {result[0].map((cell, idx) => (
                  <TableHead
                    key={idx}
                    className="h-8 px-3 font-semibold uppercase text-xs border-r last:border-r-0 bg-muted/50"
                  >
                    {cell}
                  </TableHead>
                ))}
              </TableRow>
            </TableHeader>
            <TableBody>
              {result.slice(1).map((row, rowIdx) => (
                <TableRow key={rowIdx} className="border-b hover:bg-muted/30">
                  {row.map((cell, cellIdx) => (
                    <TableCell
                      key={cellIdx}
                      className="h-7 px-3 py-1 border-r last:border-r-0"
                    >
                      {cell}
                    </TableCell>
                  ))}
                </TableRow>
              ))}
            </TableBody>
          </Table>
        ) : (
          <EmptyState
            className="h-full"
            title="No results to display"
            description="Run the query to see the results"
          />
        )}
      </div>
    </div>
  );
};

export default Results;
