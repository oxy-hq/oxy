import React from "react";
import { SidebarMenuSub } from "@/components/ui/shadcn/sidebar";
import { SchemaTreeItem } from "./SchemaTreeItem";
import type { DatabaseConnection } from "../types";

interface ConnectionSchemaContentProps {
  connection: DatabaseConnection;
  isSyncing: boolean;
  syncError?: string;
  handleSyncDatabase: (e: React.MouseEvent, databaseName: string) => void;
}

export const ConnectionSchemaContent: React.FC<
  ConnectionSchemaContentProps
> = ({ connection, isSyncing, syncError, handleSyncDatabase }) => {
  if (syncError) {
    return (
      <SidebarMenuSub className="border-l-0 ml-4">
        <div className="px-2 py-2 text-xs">
          <div className="text-destructive mb-1">Sync failed: {syncError}</div>
          <button
            onClick={(e) => handleSyncDatabase(e, connection.name)}
            className="text-primary hover:underline"
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
      <SidebarMenuSub className="border-l-0 ml-4">
        <div className="px-2 py-2 text-xs text-muted-foreground italic">
          Not synced.{" "}
          <button
            onClick={(e) => handleSyncDatabase(e, connection.name)}
            className="text-primary hover:underline"
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
      <SidebarMenuSub className="border-l-0 ml-4">
        <div className="px-2 py-2 text-xs text-muted-foreground italic">
          No tables found
        </div>
      </SidebarMenuSub>
    );
  }

  return (
    <SidebarMenuSub className="border-l-0 ml-4">
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
