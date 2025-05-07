import { useEffect, useState } from "react";
import {
  Table as DataTable,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/shadcn/table";
import { getDuckDB } from "@/libs/duckdb";
import { DataContainer, TableData, TableDisplay } from "@/types/app";
import { getData, getDataFileUrl } from "./utils";

const load_table = async (filePath: string) => {
  const db = await getDuckDB();
  const conn = await db.connect();
  const file_name = `${btoa(filePath)}.parquet`;
  await db.registerFileURL(file_name, getDataFileUrl(filePath), 4, true);
  const rs = await conn.query(`select * from "${file_name}"`);
  return rs;
};

export const DataTableBlock = ({
  display,
  data,
}: {
  display: TableDisplay;
  data: DataContainer;
}) => {
  const value = getData(data, display.data) as TableData;
  const [isLoading, setIsLoading] = useState(true);
  const [table, setTable] = useState<Awaited<
    ReturnType<typeof load_table>
  > | null>(null);

  useEffect(() => {
    (async () => {
      const table = await load_table(value.file_path);
      setTable(table);
      setIsLoading(false);
    })();
  }, [value.file_path]);

  if (isLoading)
    return (
      <div className="w-full h-full flex items-center justify-center">
        Loading...
      </div>
    );
  if (!table) return <div>No data</div>;

  return (
    <div className="flex flex-col gap-4 items-left">
      <h2>{display.title}</h2>
      <DataTable className="border">
        <TableHeader>
          <TableRow>
            {table.schema.fields.map((field) => (
              <TableHead className="text-gray-500 border" key={field.name}>
                {field.name}
              </TableHead>
            ))}
          </TableRow>
        </TableHeader>
        <TableBody>
          {table.toArray().map((row, idx) => (
            <TableRow key={idx} className="border">
              {table.schema.fields.map((field) => (
                <TableCell className="border" key={field.name}>
                  {row[field.name]}
                </TableCell>
              ))}
            </TableRow>
          ))}
        </TableBody>
      </DataTable>
    </div>
  );
};
