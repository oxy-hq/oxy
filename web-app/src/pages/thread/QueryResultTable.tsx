import {
  Table,
  TableBody,
  TableCaption,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/shadcn/table";

type ResultTableProps = {
  result: string[][];
  isTruncated: boolean;
};

export const QueryResultTable = ({ result, isTruncated }: ResultTableProps) => {
  return (
    <div className="max-h-80 overflow-y-scroll scroll w-full">
      <Table>
        <TableHeader>
          <TableRow>
            {result[0].map((col, index) => (
              <TableHead
                className="text-muted-foreground font-medium"
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
        {isTruncated && <TableCaption>Large result truncated ...</TableCaption>}
      </Table>
    </div>
  );
};
