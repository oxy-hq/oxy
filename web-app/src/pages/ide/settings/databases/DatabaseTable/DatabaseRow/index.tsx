import type React from "react";
import { useCallback } from "react";
import { TableCell, TableRow } from "@/components/ui/shadcn/table";
import { useDatabaseSync } from "@/hooks/api/databases/useDatabaseSync";
import useDatabaseOperation from "@/stores/useDatabaseOperation";
import type { DatabaseInfo as Database } from "@/types/database";
import Actions from "./Actions";
import { DatabaseInfo, DatasetInfo } from "./Info";

interface DatabaseTableRowProps {
  database: Database;
}

export const DatabaseRow: React.FC<DatabaseTableRowProps> = ({ database }) => {
  const syncMutation = useDatabaseSync();
  const handleSync = useCallback(
    (datasets?: string[]) => {
      syncMutation.mutate({
        database: database.name,
        options: {
          ...(datasets && datasets.length > 0 && { datasets })
        }
      });
    },
    [database.name, syncMutation]
  );
  const { isSyncing } = useDatabaseOperation();
  const isCurrentlySyncing = isSyncing(database.name);

  return (
    <TableRow>
      <TableCell className='font-medium'>
        <DatabaseInfo database={database} />
      </TableCell>
      <TableCell>
        <DatasetInfo datasets={database.datasets} />
      </TableCell>
      <TableCell className='max-w-md'>
        <Actions database={database} isLoading={isCurrentlySyncing} onSync={handleSync} />
      </TableCell>
    </TableRow>
  );
};

export default DatabaseRow;
