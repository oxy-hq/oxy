import { get } from "lodash";
import type React from "react";
import ErrorAlert from "@/components/ui/ErrorAlert";
import { SidebarMenuSub } from "@/components/ui/shadcn/sidebar";
import { Spinner } from "@/components/ui/shadcn/spinner";
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
      <SidebarMenuSub className='ml-[15px]'>
        <div className='px-2 py-2 text-xs'>
          <ErrorAlert message={`Sync failed: ${syncError}`} className='mb-1' />
          <button
            onClick={(e) => handleSyncDatabase(e, database.name)}
            className='text-primary hover:underline'
            disabled={isSyncing}
          >
            {isSyncing ? <Spinner className='size-2.5' /> : "Retry"}
          </button>
        </div>
      </SidebarMenuSub>
    );
  }

  if (!database.synced) {
    return (
      <SidebarMenuSub className='ml-[15px]'>
        <div className='px-2 py-2 text-muted-foreground text-xs italic'>
          Not synced.{" "}
          <button
            onClick={(e) => handleSyncDatabase(e, database.name)}
            className='text-primary hover:underline'
            disabled={isSyncing}
          >
            {isSyncing ? <Spinner className='size-2.5' /> : "Sync now"}
          </button>
        </div>
      </SidebarMenuSub>
    );
  }

  const filteredSchemas = getFilteredSchemas(database);

  if (filteredSchemas.length === 0) {
    return (
      <SidebarMenuSub className='ml-[15px]'>
        <div className='px-2 py-2 text-muted-foreground text-xs italic'>No tables found</div>
      </SidebarMenuSub>
    );
  }

  return (
    <SidebarMenuSub className='ml-[15px]'>
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
