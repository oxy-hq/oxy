import { ChevronDown, ChevronRight, Database as DatabaseIcon, RotateCw } from "lucide-react";
import React from "react";
import {
  BigQueryIcon,
  ClickHouseIcon,
  DuckDBIcon,
  MysqlIcon,
  PostgresIcon,
  RedshiftIcon,
  SnowflakeIcon
} from "@/components/icons";
import DomoIcon from "@/components/icons/Domoicon";
import { Button } from "@/components/ui/shadcn/button";
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger
} from "@/components/ui/shadcn/collapsible";
import { SidebarMenuItem } from "@/components/ui/shadcn/sidebar";
import { useDatabaseSync } from "@/hooks/api/databases/useDatabaseSync";
import { cn } from "@/libs/shadcn/utils";
import type { DatabaseInfo } from "@/types/database";
import { ConnectionSchemaContent } from "./ConnectionSchemaContent";

const getDatabaseIcon = (dialect: string) => {
  const iconProps = { className: "h-4 w-4 text-muted-foreground" };

  switch (dialect.toLowerCase()) {
    case "bigquery":
      return <BigQueryIcon {...iconProps} width={16} height={16} />;
    case "postgres":
    case "postgresql":
      return <PostgresIcon {...iconProps} width={16} height={16} />;
    case "mysql":
      return <MysqlIcon {...iconProps} width={16} height={16} />;
    case "snowflake":
      return <SnowflakeIcon {...iconProps} width={16} height={16} />;
    case "clickhouse":
      return <ClickHouseIcon {...iconProps} width={16} height={16} />;
    case "duckdb":
      return <DuckDBIcon {...iconProps} width={16} height={16} />;
    case "redshift":
      return <RedshiftIcon {...iconProps} width={16} height={16} />;
    case "domo":
      return <DomoIcon {...iconProps} width={16} height={16} />;
    default:
      return <DatabaseIcon className='h-4 w-4 text-muted-foreground' />;
  }
};

interface ConnectionItemProps {
  database: DatabaseInfo;
  isActive: boolean;
  onSelect: () => void;
}

export const ConnectionItem: React.FC<ConnectionItemProps> = ({ database, isActive, onSelect }) => {
  const [isOpen, setIsOpen] = React.useState(false);

  const syncMutation = useDatabaseSync();

  const handleSyncClick = (e: React.MouseEvent) => {
    e.stopPropagation();
    syncMutation.mutate({ database: database.name });
  };

  const connection = {
    id: database.name.toLowerCase(),
    name: database.name,
    type: database.dialect,
    synced: database.synced,
    schemas: Object.entries(database.datasets).map(([schemaName, tables]) => ({
      name: schemaName,
      tables: tables.map((tableName) => ({ name: tableName }))
    }))
  };

  return (
    <Collapsible open={isOpen} onOpenChange={setIsOpen}>
      <SidebarMenuItem>
        <CollapsibleTrigger asChild>
          <div
            className={cn(
              "group flex w-full cursor-pointer items-center gap-1 rounded-md px-2 py-1",
              isActive
                ? "bg-sidebar-accent text-sidebar-accent-foreground"
                : "hover:bg-sidebar-accent/50"
            )}
            onClick={onSelect}
          >
            <Button variant='ghost' size='icon' className='h-4 w-4 p-0'>
              {isOpen ? <ChevronDown className='h-3 w-3' /> : <ChevronRight className='h-3 w-3' />}
            </Button>
            {getDatabaseIcon(database.dialect)}
            <span className='flex-1 truncate text-sm'>{database.name}</span>
            <Button
              variant='ghost'
              size='icon'
              className='h-5 w-5 p-0 opacity-0 group-hover:opacity-100'
              onClick={handleSyncClick}
              disabled={syncMutation.isPending}
              tooltip='Sync Schema'
            >
              <RotateCw className={cn("h-3 w-3", syncMutation.isPending && "animate-spin")} />
            </Button>
          </div>
        </CollapsibleTrigger>
        <CollapsibleContent>
          <ConnectionSchemaContent
            connection={connection}
            isSyncing={syncMutation.isPending}
            syncError={syncMutation.isError ? syncMutation.error?.message : undefined}
            handleSyncDatabase={handleSyncClick}
          />
        </CollapsibleContent>
      </SidebarMenuItem>
    </Collapsible>
  );
};
