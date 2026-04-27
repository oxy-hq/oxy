import { ChevronDown, ChevronRight, Columns, Table } from "lucide-react";
import React from "react";
import { useNavigate } from "react-router-dom";
import { toast } from "sonner";
import {
  SidebarMenuSub,
  SidebarMenuSubButton,
  SidebarMenuSubItem
} from "@/components/ui/shadcn/sidebar";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import ROUTES from "@/libs/utils/routes";
import useCurrentOrg from "@/stores/useCurrentOrg";
import useDatabaseClient from "@/stores/useDatabaseClient";
import type { TableInfo } from "@/types/database";

const generateSelectQuery = (tableName: string, dialect?: string): string => {
  const d = dialect?.toLowerCase() ?? "";
  switch (d) {
    case "mysql":
      return `SELECT * FROM \`${tableName}\`\nLIMIT 100;`;
    case "postgres":
    case "postgresql":
    case "redshift":
    case "snowflake":
      return `SELECT * FROM "${tableName}"\nLIMIT 100;`;
    default:
      return `SELECT * FROM ${tableName}\nLIMIT 100;`;
  }
};

interface TableRowProps {
  table: TableInfo;
  dialect: string;
  databaseName: string;
}

const TableRow: React.FC<TableRowProps> = ({ table, dialect, databaseName }) => {
  const [isOpen, setIsOpen] = React.useState(false);
  const { addTab } = useDatabaseClient();
  const navigate = useNavigate();
  const { project } = useCurrentProjectBranch();
  const projectId = project.id;
  const orgSlug = useCurrentOrg((s) => s.org?.slug) ?? "";

  const handleTableClick = (e: React.MouseEvent) => {
    e.stopPropagation();
    const result = addTab({
      name: `${table.name}.sql`,
      content: generateSelectQuery(table.name, dialect),
      isDirty: true,
      selectedDatabase: databaseName
    });
    if (!result.success) {
      toast.error(result.error);
    }
    navigate(ROUTES.ORG(orgSlug).WORKSPACE(projectId).IDE.DATABASE.ROOT);
  };

  const hasColumns = table.columns.length > 0;

  return (
    <SidebarMenuSubItem>
      <SidebarMenuSubButton
        onClick={() => setIsOpen((open) => !open)}
        className='text-muted-foreground hover:text-sidebar-foreground'
      >
        {hasColumns ? (
          isOpen ? (
            <ChevronDown className='h-3 w-3 shrink-0' />
          ) : (
            <ChevronRight className='h-3 w-3 shrink-0' />
          )
        ) : (
          <span className='w-3 shrink-0' />
        )}
        <Table className='h-3.5 w-3.5 shrink-0' />
        <span className='truncate' onClick={handleTableClick}>
          {table.name}
        </span>
      </SidebarMenuSubButton>

      {isOpen && hasColumns && (
        <SidebarMenuSub className='ml-[15px]'>
          {table.columns.map((col) => (
            <SidebarMenuSubItem key={col.name}>
              <div className='flex items-center gap-1.5 px-2 py-0.5 text-muted-foreground text-xs'>
                <Columns className='h-3 w-3 shrink-0' />
                <span className='truncate'>{col.name}</span>
                <span className='ml-auto shrink-0 text-muted-foreground/60'>{col.data_type}</span>
              </div>
            </SidebarMenuSubItem>
          ))}
        </SidebarMenuSub>
      )}
    </SidebarMenuSubItem>
  );
};

interface SchemaTableListProps {
  tables: TableInfo[];
  dialect: string;
  databaseName: string;
}

export const SchemaTableList: React.FC<SchemaTableListProps> = ({
  tables,
  dialect,
  databaseName
}) => {
  const sorted = [...tables].sort((a, b) => a.name.localeCompare(b.name));
  return (
    <>
      {sorted.map((table) => (
        <TableRow key={table.name} table={table} dialect={dialect} databaseName={databaseName} />
      ))}
    </>
  );
};
