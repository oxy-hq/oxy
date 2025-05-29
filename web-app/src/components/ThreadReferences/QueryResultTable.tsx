import {
  Table,
  TableBody,
  TableCaption,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/shadcn/table";
import { Button } from "../ui/shadcn/button";
import Papa from "papaparse";
import { Table as TableIcon } from "lucide-react";

type ResultTableProps = {
  result: string[][];
  isTruncated: boolean;
};

export const QueryResultTable = ({ result, isTruncated }: ResultTableProps) => {
  const handleDownloadCsv = () => {
    const csvContent = Papa.unparse(result, {
      delimiter: ",",
      header: true,
      skipEmptyLines: true,
    });
    const blob = new Blob([csvContent], { type: "text/csv;charset=utf-8;" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = "query_result.csv";
    a.click();
    URL.revokeObjectURL(url);
  };

  return (
    <div className="flex flex-col gap-4">
      <div className="max-h-80 overflow-auto customScrollbar">
        <Table className="w-full">
          <TableHeader>
            <TableRow>
              {result[0].map((col, index) => (
                <TableHead
                  className="text-muted-foreground font-medium min-w-32"
                  key={index}
                >
                  {col}
                </TableHead>
              ))}
            </TableRow>
          </TableHeader>
          <TableBody>
            {result.slice(1).map((row, rowIndex) => (
              <TableRow key={rowIndex}>
                {row.map((cell, cellIndex) => (
                  <TableCell key={cellIndex}>{cell}</TableCell>
                ))}
              </TableRow>
            ))}
          </TableBody>
          {isTruncated && (
            <TableCaption>Large result truncated ...</TableCaption>
          )}
        </Table>
      </div>
      <div className="flex items-center gap-2 justify-end">
        <Button
          variant="outline"
          title="Download CSV"
          onClick={handleDownloadCsv}
        >
          <TableIcon className="mr-2 h-4 w-4" />
          Download the result
        </Button>
      </div>
    </div>
  );
};
