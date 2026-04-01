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
import { SidebarMenuButton, SidebarMenuItem } from "@/components/ui/shadcn/sidebar";
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
}

export const ConnectionItem: React.FC<ConnectionItemProps> = ({ database }) => {
  const [isOpen, setIsOpen] = React.useState(false);

  const syncMutation = useDatabaseSync();

  const handleSyncClick = (e: React.MouseEvent) => {
    e.stopPropagation();
    syncMutation.mutate({ database: database.name });
  };

  const Chevron = isOpen ? ChevronDown : ChevronRight;

  return (
    <SidebarMenuItem>
      <SidebarMenuButton
        onClick={() => setIsOpen((open) => !open)}
        className='text-sidebar-foreground hover:bg-sidebar-accent'
      >
        <Chevron className='h-4 w-4' />
        {getDatabaseIcon(database.dialect)}
        <span className='flex-1 truncate'>{database.name}</span>

        <Button
          variant='ghost'
          size='icon'
          className='opacity-0 group-hover/menu-item:opacity-100'
          onClick={handleSyncClick}
          disabled={syncMutation.isPending}
          tooltip='Sync Schema'
        >
          <RotateCw className={cn(syncMutation.isPending && "animate-spin")} />
        </Button>
      </SidebarMenuButton>

      {isOpen && (
        <ConnectionSchemaContent
          database={database}
          isSyncing={syncMutation.isPending}
          syncError={syncMutation.isError ? syncMutation.error?.message : undefined}
          handleSyncDatabase={handleSyncClick}
        />
      )}
    </SidebarMenuItem>
  );
};
