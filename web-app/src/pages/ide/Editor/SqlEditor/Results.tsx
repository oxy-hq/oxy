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
    <>
      {result.length > 0 && (
        <div className="flex flex-col h-full overflow-hidden">
          <div className="h-[35px] flex items-center p-3">
            <p className="text-sm font-medium">Results</p>
          </div>
          <div className="flex-1 overflow-auto customScrollbar p-2">
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
          </div>
        </div>
      )}
    </>
  );
};

export default Results;
