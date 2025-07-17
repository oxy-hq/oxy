import React from "react";
import {
  Table,
  TableBody,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/shadcn/table";
import DatabaseRow from "./DatabaseRow";
import useDatabases from "@/hooks/api/databases/useDatabases";
import TableContentWrapper from "../../components/TableContentWrapper";
import TableWrapper from "../../components/TableWrapper";

export const DatabaseTable: React.FC = () => {
  const { data: databases = [], isLoading, error, refetch } = useDatabases();

  return (
    <TableWrapper>
      <Table>
        <TableHeader>
          <TableRow>
            <TableHead>Database</TableHead>
            <TableHead>Datasets</TableHead>
            <TableHead>Actions</TableHead>
          </TableRow>
        </TableHeader>
        <TableBody>
          <TableContentWrapper
            loading={isLoading}
            error={error?.message}
            colSpan={3}
            isEmpty={databases.length === 0}
            noFoundTitle="No databases found"
            noFoundDescription="Add a database to get started"
            onRetry={refetch}
          >
            {databases.map((database) => (
              <DatabaseRow key={database.name} database={database} />
            ))}
          </TableContentWrapper>
        </TableBody>
      </Table>
    </TableWrapper>
  );
};

export default DatabaseTable;
