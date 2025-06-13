import EmptyState from "@/components/ui/EmptyState";
import { Button } from "@/components/ui/shadcn/button";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/shadcn/table";
import Papa from "papaparse";
import { TableIcon } from "lucide-react";
import { handleDownloadFile } from "@/libs/utils/string";

interface ResultsProps {
  result: string[][];
}

const Results = ({ result }: ResultsProps) => {
  const handleDownloadCsv = () => {
    const csvContent = Papa.unparse(result, {
      delimiter: ",",
      header: true,
      skipEmptyLines: true,
    });
    const blob = new Blob([csvContent], { type: "text/csv;charset=utf-8;" });
    handleDownloadFile(blob, "query_result.sql");
  };

  return (
    <div className="flex flex-col h-full overflow-hidden">
      <div className="h-[35px] flex items-center px-3 py-2">
        <p className="text-sm font-medium flex-1">Results</p>
        {result.length > 0 && (
          <Button
            variant="outline"
            title="Download CSV"
            onClick={handleDownloadCsv}
          >
            <TableIcon className="mr-2 h-4 w-4" />
            Download the result
          </Button>
        )}
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
