import { useEffect, useState } from "react";
import {
  Table as DataTable,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow
} from "@/components/ui/shadcn/table";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { getDuckDB } from "@/libs/duckdb";
import type { DataContainer, TableData, TableDisplay } from "@/types/app";
import {
  getArrowFieldType,
  getArrowValueWithType,
  getData,
  registerAuthenticatedFile
} from "./utils";

const load_table = async (filePath: string, projectId: string, branchName: string) => {
  const db = await getDuckDB();
  const conn = await db.connect();
  const file_name = await registerAuthenticatedFile(filePath, projectId, branchName);
  const rs = await conn.query(`select * from "${file_name}"`);
  return rs;
};

export const DataTableBlock = ({
  display,
  data
}: {
  display: TableDisplay;
  data?: DataContainer;
}) => {
  const [isLoading, setIsLoading] = useState(true);
  const { project, branchName } = useCurrentProjectBranch();
  const [table, setTable] = useState<Awaited<ReturnType<typeof load_table>> | null>(null);

  const dataAvailable = data && display.data;

  useEffect(() => {
    (async () => {
      if (!dataAvailable) {
        setTable(null);
        setIsLoading(false);
        return;
      }
      const value = getData(data, display.data) as TableData | null;
      if (!value) {
        setTable(null);
        setIsLoading(false);
        return;
      }

      const table = await load_table(value.file_path, project.id, branchName);
      setTable(table);
      setIsLoading(false);
    })();
  }, [branchName, data, dataAvailable, display.data, project.id]);

  if (isLoading)
    return <div className='flex h-full w-full items-center justify-center'>Loading...</div>;

  let tableContent;
  if (!table) {
    tableContent = <div className='p-2 text-center text-gray-500'>No data found</div>;
  } else {
    tableContent = (
      <DataTable className='border'>
        <TableHeader>
          <TableRow>
            {table.schema.fields.map((field) => (
              <TableHead className='border text-gray-500' key={field.name}>
                {field.name}
              </TableHead>
            ))}
          </TableRow>
        </TableHeader>
        <TableBody>
          {table.toArray().map((row, idx) => (
            <TableRow key={idx} className='border'>
              {table.schema.fields.map((field) => {
                const fieldType = getArrowFieldType(field.name, table.schema);
                const value = row[field.name];
                const formattedValue = fieldType ? getArrowValueWithType(value, fieldType) : value;
                return (
                  <TableCell className='border' key={field.name}>
                    {String(formattedValue)}
                  </TableCell>
                );
              })}
            </TableRow>
          ))}
        </TableBody>
      </DataTable>
    );
  }

  return (
    <div className='items-left flex flex-col gap-4' data-testid='app-data-table-block'>
      <h2 className='font-bold text-foreground text-xl'>{display.title}</h2>
      {tableContent}
    </div>
  );
};
