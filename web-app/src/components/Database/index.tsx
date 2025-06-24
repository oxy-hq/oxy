import React, { useCallback } from "react";
import {
  Table,
  TableBody,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/shadcn/table";
import { DatabaseInfo } from "@/types/database";
import {
  DatabaseTableRow,
  DatabaseTableLoading,
  DatabaseTableEmpty,
} from "./Table";
import { useDatabaseSync } from "@/hooks/api/useDatabaseSync";

interface DatabaseTableProps {
  databases: DatabaseInfo[];
  loading: boolean;
}

export const DatabaseTable: React.FC<DatabaseTableProps> = ({
  databases,
  loading,
}) => {
  const syncMutation = useDatabaseSync();

  const handleSyncDatabase = useCallback(
    (database: DatabaseInfo, datasets?: string[]) => {
      syncMutation.mutate({
        database: database.name,
        options: {
          ...(datasets && datasets.length > 0 && { datasets }),
        },
      });
    },
    [syncMutation],
  );

  if (loading) {
    return <DatabaseTableLoading />;
  }

  if (databases.length === 0) {
    return <DatabaseTableEmpty />;
  }

  return (
    <div className="border rounded-lg">
      <Table>
        <TableHeader>
          <TableRow>
            <TableHead className="w-[400px]">Database</TableHead>
            <TableHead className="w-[250px]">Datasets</TableHead>
            <TableHead className="w-[120px] text-right">Actions</TableHead>
          </TableRow>
        </TableHeader>
        <TableBody>
          {databases.map((database) => (
            <DatabaseTableRow
              key={database.name}
              database={database}
              onSyncDatabase={handleSyncDatabase}
            />
          ))}
        </TableBody>
      </Table>
    </div>
  );
};

export default DatabaseTable;
export { EmbeddingsManagement } from "./Management";
