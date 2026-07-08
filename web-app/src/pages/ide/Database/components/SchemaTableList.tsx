import { ChevronDown, ChevronRight, Columns, Database, Table, Trash2 } from "lucide-react";
import React from "react";
import { useNavigate } from "react-router-dom";
import { toast } from "sonner";
import { CanWorkspaceEditor } from "@/components/auth/Can";
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuTrigger
} from "@/components/ui/shadcn/context-menu";
import {
  SidebarMenuSub,
  SidebarMenuSubButton,
  SidebarMenuSubItem
} from "@/components/ui/shadcn/sidebar";
import useDropDbObject from "@/hooks/api/databases/useDropDbObject";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import ROUTES from "@/libs/utils/routes";
import useCurrentOrg from "@/stores/useCurrentOrg";
import useDatabaseClient from "@/stores/useDatabaseClient";
import type { TableInfo } from "@/types/database";
import DropObjectDialog from "./DropObjectDialog";
import { parseQualifiedName, quoteIdent } from "./dbIdent";

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
  /** Bare table label to show; falls back to the full (possibly qualified) name. */
  label?: string;
}

const TableRow: React.FC<TableRowProps> = ({ table, dialect, databaseName, label }) => {
  const [isOpen, setIsOpen] = React.useState(false);
  const [showDrop, setShowDrop] = React.useState(false);
  const { addTab } = useDatabaseClient();
  const navigate = useNavigate();
  const { project } = useCurrentProjectBranch();
  const projectId = project.id;
  const orgSlug = useCurrentOrg((s) => s.org?.slug) ?? "";
  const drop = useDropDbObject(databaseName);
  const displayLabel = label ?? table.name;

  const handleTableClick = (e: React.MouseEvent) => {
    e.stopPropagation();
    // Tab title: readable, unquoted `schema.table` (or bare `table`) instead of the
    // literal `"schema"."table"`. Keeping the schema prefix preserves uniqueness so
    // same-named tables in different schemas don't collide on the tab-name key. The
    // query body still uses the fully qualified `table.name` so it resolves.
    const { schema, label: bareLabel } = parseQualifiedName(table.name);
    const tabLabel = schema === null ? bareLabel : `${schema}.${bareLabel}`;
    const result = addTab({
      name: `${tabLabel}.sql`,
      content: generateSelectQuery(table.name, dialect),
      isDirty: true,
      selectedDatabase: databaseName
    });
    if (!result.success) {
      toast.error(result.error);
    }
    navigate(ROUTES.ORG(orgSlug).WORKSPACE(projectId).IDE.DATABASE.ROOT);
  };

  const handleDrop = async () => {
    try {
      // Schema-qualified connectors (airhouse) report `table.name` as an already
      // quoted, runnable `"schema"."table"` reference — use it as-is so the drop
      // resolves across schemas. Bare-name connectors (DuckDB/Postgres) report an
      // unquoted name, so quote it here (matching the DROP SCHEMA path) or reserved
      // words / mixed-case / special chars would fail to drop.
      const { schema } = parseQualifiedName(table.name);
      const dropRef = schema === null ? quoteIdent(table.name) : table.name;
      await drop.mutateAsync(`DROP TABLE ${dropRef}`);
      toast.success(`Dropped table ${displayLabel}`);
      setShowDrop(false);
    } catch (err) {
      toast.error(err instanceof Error ? `Drop failed: ${err.message}` : "Drop failed");
    }
  };

  const hasColumns = table.columns.length > 0;

  return (
    <SidebarMenuSubItem>
      <ContextMenu>
        <ContextMenuTrigger asChild>
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
              {displayLabel}
            </span>
          </SidebarMenuSubButton>
        </ContextMenuTrigger>
        <ContextMenuContent>
          <CanWorkspaceEditor>
            <ContextMenuItem
              className='text-destructive focus:text-destructive'
              onSelect={() => setShowDrop(true)}
            >
              <Trash2 className='h-3.5 w-3.5' />
              Drop table
            </ContextMenuItem>
          </CanWorkspaceEditor>
        </ContextMenuContent>
      </ContextMenu>

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

      <DropObjectDialog
        open={showDrop}
        onOpenChange={setShowDrop}
        kind='table'
        name={displayLabel}
        onConfirm={handleDrop}
        isPending={drop.isPending}
      />
    </SidebarMenuSubItem>
  );
};

interface SchemaGroupProps {
  schema: string;
  tables: TableInfo[];
  dialect: string;
  databaseName: string;
}

const SchemaGroup: React.FC<SchemaGroupProps> = ({ schema, tables, dialect, databaseName }) => {
  const [isOpen, setIsOpen] = React.useState(true);
  const [showDrop, setShowDrop] = React.useState(false);
  const drop = useDropDbObject(databaseName);

  const handleDrop = async () => {
    try {
      await drop.mutateAsync(`DROP SCHEMA ${quoteIdent(schema)} CASCADE`);
      toast.success(`Dropped schema ${schema}`);
      setShowDrop(false);
    } catch (err) {
      toast.error(err instanceof Error ? `Drop failed: ${err.message}` : "Drop failed");
    }
  };

  const sorted = [...tables].sort((a, b) => a.name.localeCompare(b.name));

  return (
    <SidebarMenuSubItem>
      <ContextMenu>
        <ContextMenuTrigger asChild>
          <SidebarMenuSubButton
            onClick={() => setIsOpen((open) => !open)}
            className='text-sidebar-foreground'
          >
            {isOpen ? (
              <ChevronDown className='h-3 w-3 shrink-0' />
            ) : (
              <ChevronRight className='h-3 w-3 shrink-0' />
            )}
            <Database className='h-3.5 w-3.5 shrink-0' />
            <span className='truncate font-medium'>{schema}</span>
            <span className='ml-auto shrink-0 text-muted-foreground/60 text-xs'>
              {tables.length}
            </span>
          </SidebarMenuSubButton>
        </ContextMenuTrigger>
        <ContextMenuContent>
          <CanWorkspaceEditor>
            <ContextMenuItem
              className='text-destructive focus:text-destructive'
              onSelect={() => setShowDrop(true)}
            >
              <Trash2 className='h-3.5 w-3.5' />
              Drop schema
            </ContextMenuItem>
          </CanWorkspaceEditor>
        </ContextMenuContent>
      </ContextMenu>

      {isOpen && (
        <SidebarMenuSub className='ml-[15px]'>
          {sorted.map((table) => (
            <TableRow
              key={table.name}
              table={table}
              dialect={dialect}
              databaseName={databaseName}
              label={parseQualifiedName(table.name).label}
            />
          ))}
        </SidebarMenuSub>
      )}

      <DropObjectDialog
        open={showDrop}
        onOpenChange={setShowDrop}
        kind='schema'
        name={schema}
        onConfirm={handleDrop}
        isPending={drop.isPending}
      />
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
  // Group by the schema parsed from each table's (possibly qualified) name.
  // Connectors that return bare names (no schema) land in the "" bucket and
  // render flat — preserving the old behavior for non-airhouse connections.
  const groups = new Map<string, TableInfo[]>();
  for (const table of tables) {
    const key = parseQualifiedName(table.name).schema ?? "";
    const bucket = groups.get(key);
    if (bucket) bucket.push(table);
    else groups.set(key, [table]);
  }

  const hasSchemas = [...groups.keys()].some((k) => k !== "");
  if (!hasSchemas) {
    const sorted = [...tables].sort((a, b) => a.name.localeCompare(b.name));
    return (
      <>
        {sorted.map((table) => (
          <TableRow key={table.name} table={table} dialect={dialect} databaseName={databaseName} />
        ))}
      </>
    );
  }

  const schemaNames = [...groups.keys()].sort((a, b) => a.localeCompare(b));
  return (
    <>
      {schemaNames.map((schema) =>
        schema === "" ? (
          // Bare-name tables (no schema): render flat, no group header.
          <React.Fragment key='__ungrouped'>
            {(groups.get(schema) ?? []).map((table) => (
              <TableRow
                key={table.name}
                table={table}
                dialect={dialect}
                databaseName={databaseName}
              />
            ))}
          </React.Fragment>
        ) : (
          <SchemaGroup
            key={schema}
            schema={schema}
            tables={groups.get(schema) ?? []}
            dialect={dialect}
            databaseName={databaseName}
          />
        )
      )}
    </>
  );
};
