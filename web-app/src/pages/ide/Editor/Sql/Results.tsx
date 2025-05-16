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
}

const Results = ({ result }: ResultsProps) => {
  return (
    <div className="flex flex-col h-full overflow-hidden">
      <div className="h-[35px] flex items-center p-3">
        <p className="text-sm font-medium">Results</p>
      </div>
      <div className="flex-1 overflow-auto customScrollbar p-2">
        {result.length > 0 ? (
          <Table className="border">
            <TableHeader>
              <TableRow>
                {result[0].map((cell) => (
                  <TableHead className="border-r">{cell}</TableHead>
                ))}
              </TableRow>
            </TableHeader>
            <TableBody>
              {result.slice(1).map((row) => (
                <TableRow className="border-b">
                  {row.map((cell) => (
                    <TableCell className="border-r">{cell}</TableCell>
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
