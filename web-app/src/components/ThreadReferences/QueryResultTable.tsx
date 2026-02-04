import { Table as TableIcon } from "lucide-react";
import Papa from "papaparse";
import {
  Table,
  TableBody,
  TableCaption,
  TableCell,
  TableHead,
  TableHeader,
  TableRow
} from "@/components/ui/shadcn/table";
import { Button } from "../ui/shadcn/button";
import { VirtualizedTable } from "../ui/VirtualizedTable";

type ResultTableProps = {
  result?: string[][];
  resultFile?: string;
  isTruncated: boolean;
};

export const QueryResultTable = ({ result, resultFile, isTruncated }: ResultTableProps) => {
  // If we have a result file, use the virtualized table
  if (resultFile) {
    return (
      <div className='flex flex-col gap-4'>
        <VirtualizedTable filePath={resultFile} pageSize={100} key={resultFile} maxHeight='32rem' />
        {isTruncated && (
          <p className='text-center text-muted-foreground text-sm'>
            Large result set loaded via Arrow format
          </p>
        )}
      </div>
    );
  }

  // Fallback to array-based rendering for backward compatibility
  if (!result || result.length === 0) {
    return <div className='p-4 text-center text-muted-foreground'>No results to display</div>;
  }

  const handleDownloadCsv = () => {
    const csvContent = Papa.unparse(result, {
      delimiter: ",",
      header: true,
      skipEmptyLines: true
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
    <div className='flex flex-col gap-4'>
      <div className='customScrollbar max-h-80 overflow-auto'>
        <Table className='w-full'>
          <TableHeader>
            <TableRow>
              {result[0].map((col, index) => (
                <TableHead className='min-w-32 font-medium text-muted-foreground' key={index}>
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
          {isTruncated && <TableCaption>Large result truncated ...</TableCaption>}
        </Table>
      </div>
      <div className='flex items-center justify-end gap-2'>
        <Button variant='outline' title='Download CSV' onClick={handleDownloadCsv}>
          <TableIcon className='mr-2 h-4 w-4' />
          Download the result
        </Button>
      </div>
    </div>
  );
};
