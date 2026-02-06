import { get } from "lodash";
import type React from "react";
import { SidebarMenuSub } from "@/components/ui/shadcn/sidebar";
import type { DatabaseInfo } from "@/types/database";
import { SchemaTreeItem } from "./SchemaTreeItem";

const IGNORE_METADATA_SCHEMA_PREFIX = "__ducklake";

const getFilteredSchemas = (database: DatabaseInfo) => {
  const datasets = get(database, "datasets");
  if (!datasets) return [];
  return Object.entries(database.datasets)
    .filter(([, tables]) =>
      Object.values(tables ?? {}).some(
        (semantic) => !get(semantic, "database_name", "")?.startsWith(IGNORE_METADATA_SCHEMA_PREFIX)
      )
    )
    .map(([schemaName, tables]) => ({ name: schemaName, tables }));
};

interface ConnectionSchemaContentProps {
  database: DatabaseInfo;
  isSyncing: boolean;
  syncError?: string;
  handleSyncDatabase: (e: React.MouseEvent, databaseName: string) => void;
}

export const ConnectionSchemaContent: React.FC<ConnectionSchemaContentProps> = ({
  database,
  isSyncing,
  syncError,
  handleSyncDatabase
}) => {
  if (syncError) {
    return (
      <SidebarMenuSub className='ml-4'>
        <div className='px-2 py-2 text-xs'>
          <div className='mb-1 text-destructive'>Sync failed: {syncError}</div>
          <button
            onClick={(e) => handleSyncDatabase(e, database.name)}
            className='text-primary hover:underline'
            disabled={isSyncing}
          >
            {isSyncing ? "Syncing..." : "Retry"}
          </button>
        </div>
      </SidebarMenuSub>
    );
  }

  if (!database.synced) {
    return (
      <SidebarMenuSub className='ml-4'>
        <div className='px-2 py-2 text-muted-foreground text-xs italic'>
          Not synced.{" "}
          <button
            onClick={(e) => handleSyncDatabase(e, database.name)}
            className='text-primary hover:underline'
            disabled={isSyncing}
          >
            {isSyncing ? "Syncing..." : "Sync now"}
          </button>
        </div>
      </SidebarMenuSub>
    );
  }

  const filteredSchemas = getFilteredSchemas(database);

  if (filteredSchemas.length === 0) {
    return (
      <SidebarMenuSub className='ml-4'>
        <div className='px-2 py-2 text-muted-foreground text-xs italic'>No tables found</div>
      </SidebarMenuSub>
    );
  }

  return (
    <SidebarMenuSub className='ml-4'>
      {filteredSchemas
        .sort((a, b) => a.name.localeCompare(b.name))
        .map((schema) => (
          <SchemaTreeItem
            key={`${database.name}-${schema.name}`}
            schemaName={schema.name}
            dialect={database.dialect}
            databaseName={database.name}
            tables={schema.tables}
          />
        ))}
    </SidebarMenuSub>
  );
};
