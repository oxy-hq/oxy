import React from "react";
import { TableCell, TableRow } from "@/components/ui/shadcn/table";
import { DatabaseInfo } from "@/types/database";
import useDatabaseOperation from "@/stores/useDatabaseOperation";
import {
  DatabaseInfoDisplay,
  DatasetInfoDisplay,
  DatabaseActions,
} from "../Management";

interface DatabaseTableRowProps {
  database: DatabaseInfo;
  onSyncDatabase: (database: DatabaseInfo, datasets?: string[]) => void;
}

export const DatabaseTableRow: React.FC<DatabaseTableRowProps> = ({
  database,
  onSyncDatabase,
}) => {
  const { isSyncing } = useDatabaseOperation();
  const isCurrentlySyncing = isSyncing(database.name);

  const handleSync = (datasets?: string[]) => {
    onSyncDatabase(database, datasets);
  };

  return (
    <TableRow>
      <TableCell className="font-medium">
        <DatabaseInfoDisplay database={database} />
      </TableCell>
      <TableCell>
        <DatasetInfoDisplay datasets={database.datasets} />
      </TableCell>
      <TableCell className="text-right">
        <DatabaseActions
          database={database}
          isLoading={isCurrentlySyncing}
          onSync={handleSync}
        />
      </TableCell>
    </TableRow>
  );
};
