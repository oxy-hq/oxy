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
import TableWrapper from "../../components/TableWrapper";
import TableContentWrapper from "../../components/TableContentWrapper";

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
            isEmpty={databases.length === 0}
            loading={isLoading}
            colSpan={3}
            noFoundTitle="No databases found"
            noFoundDescription="Add a database to get started"
            error={error?.message}
            onRetry={() => refetch()}
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
