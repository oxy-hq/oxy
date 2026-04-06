import type React from "react";
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
import { getArrowFieldType, getArrowValueWithType, getData, registerFromTableData } from "./utils";

const load_table = async (
  tableData: { file_path: string; json?: string | null },
  projectId: string,
  branchName: string
) => {
  const db = await getDuckDB();
  const conn = await db.connect();
  try {
    const file_name = await registerFromTableData(tableData, projectId, branchName);
    return await conn.query(`select * from "${file_name}"`);
  } finally {
    await conn.close();
  }
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
    setIsLoading(true);
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
      // Empty JSON result → show "No data found" without hitting DuckDB.
      if (typeof value.json === "string" && value.json.trim() === "[]") {
        setTable(null);
        setIsLoading(false);
        return;
      }

      try {
        const table = await load_table(value, project.id, branchName);
        setTable(table);
      } catch {
        setTable(null);
      } finally {
        setIsLoading(false);
      }
    })();
  }, [branchName, data, dataAvailable, display.data, project.id]);

  if (isLoading)
    return <div className='flex h-full w-full items-center justify-center'>Loading...</div>;

  let tableContent: React.ReactNode;
  if (!table) {
    tableContent = <div className='p-2 text-center text-muted-foreground'>No data found</div>;
  } else {
    tableContent = (
      <DataTable className='border'>
        <TableHeader>
          <TableRow>
            {table.schema.fields.map((field) => (
              <TableHead className='border text-muted-foreground' key={field.name}>
                {field.name}
              </TableHead>
            ))}
          </TableRow>
        </TableHeader>
        <TableBody>
          {table.toArray().map((row, idx) => (
            // biome-ignore lint/suspicious/noArrayIndexKey: rows have no stable id
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
