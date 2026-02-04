import type React from "react";
import { SidebarMenuSub } from "@/components/ui/shadcn/sidebar";
import type { DatabaseConnection } from "../types";
import { SchemaTreeItem } from "./SchemaTreeItem";

interface ConnectionSchemaContentProps {
  connection: DatabaseConnection;
  isSyncing: boolean;
  syncError?: string;
  handleSyncDatabase: (e: React.MouseEvent, databaseName: string) => void;
}

export const ConnectionSchemaContent: React.FC<ConnectionSchemaContentProps> = ({
  connection,
  isSyncing,
  syncError,
  handleSyncDatabase
}) => {
  if (syncError) {
    return (
      <SidebarMenuSub className='ml-4 border-l-0'>
        <div className='px-2 py-2 text-xs'>
          <div className='mb-1 text-destructive'>Sync failed: {syncError}</div>
          <button
            onClick={(e) => handleSyncDatabase(e, connection.name)}
            className='text-primary hover:underline'
            disabled={isSyncing}
          >
            {isSyncing ? "Syncing..." : "Retry"}
          </button>
        </div>
      </SidebarMenuSub>
    );
  }

  if (!connection.synced) {
    return (
      <SidebarMenuSub className='ml-4 border-l-0'>
        <div className='px-2 py-2 text-muted-foreground text-xs italic'>
          Not synced.{" "}
          <button
            onClick={(e) => handleSyncDatabase(e, connection.name)}
            className='text-primary hover:underline'
            disabled={isSyncing}
          >
            {isSyncing ? "Syncing..." : "Sync now"}
          </button>
        </div>
      </SidebarMenuSub>
    );
  }

  if (!connection.schemas || connection.schemas.length === 0) {
    return (
      <SidebarMenuSub className='ml-4 border-l-0'>
        <div className='px-2 py-2 text-muted-foreground text-xs italic'>No tables found</div>
      </SidebarMenuSub>
    );
  }

  return (
    <SidebarMenuSub className='ml-4 border-l-0'>
      {connection.schemas.map((schema) => (
        <SchemaTreeItem
          key={`${connection.id}-${schema.name}`}
          schema={schema}
          dialect={connection.type}
          connectionId={connection.id}
          databaseName={connection.name}
        />
      ))}
    </SidebarMenuSub>
  );
};
