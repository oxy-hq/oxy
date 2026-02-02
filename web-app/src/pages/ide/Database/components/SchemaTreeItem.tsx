import React from "react";
import { ChevronDown, ChevronRight, Folder, Table } from "lucide-react";
import {
  SidebarMenuSub,
  SidebarMenuSubItem,
  SidebarMenuSubButton,
} from "@/components/ui/shadcn/sidebar";
import {
  Collapsible,
  CollapsibleTrigger,
  CollapsibleContent,
} from "@/components/ui/shadcn/collapsible";
import type { DatabaseSchema } from "../types";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { useNavigate } from "react-router-dom";
import useDatabaseClient from "@/stores/useDatabaseClient";
import ROUTES from "@/libs/utils/routes";
import { toast } from "sonner";

interface SchemaTreeItemProps {
  schema: DatabaseSchema;
  dialect?: string;
  connectionId?: string;
  databaseName?: string;
}

const generateSelectQuery = (
  schemaName: string,
  tableName: string,
  dialect?: string,
): string => {
  const normalizedDialect = dialect?.toLowerCase() || "";

  switch (normalizedDialect) {
    case "bigquery":
      return `SELECT * FROM \`${schemaName}.${tableName}\`\nLIMIT 100;`;
    case "mysql":
      return `SELECT * FROM \`${schemaName}\`.\`${tableName}\`\nLIMIT 100;`;
    case "postgres":
    case "postgresql":
    case "redshift":
      return `SELECT * FROM "${schemaName}"."${tableName}"\nLIMIT 100;`;
    case "snowflake":
      return `SELECT * FROM "${schemaName}"."${tableName}"\nLIMIT 100;`;
    case "clickhouse":
      return `SELECT * FROM ${schemaName}.${tableName}\nLIMIT 100;`;
    case "duckdb":
      return `SELECT * FROM "${tableName}"\nLIMIT 100;`;
    case "domo":
      return `SELECT * FROM ${tableName}\nLIMIT 100`;
    default:
      return `SELECT * FROM ${schemaName}.${tableName}\nLIMIT 100;`;
  }
};

export const SchemaTreeItem: React.FC<SchemaTreeItemProps> = ({
  schema,
  dialect,
  databaseName,
}) => {
  const [isOpen, setIsOpen] = React.useState(false);
  const { addTab } = useDatabaseClient();
  const navigate = useNavigate();
  const { project } = useCurrentProjectBranch();
  const projectId = project.id;

  const handleTableClick = (tableName: string) => {
    const result = addTab({
      name: `${tableName}.sql`,
      content: generateSelectQuery(schema.name, tableName, dialect),
      isDirty: true,
      selectedDatabase: databaseName,
    });
    if (!result.success) {
      toast.error(result.error);
    }
    navigate(ROUTES.PROJECT(projectId).IDE.DATABASE.ROOT);
  };

  return (
    <Collapsible open={isOpen} onOpenChange={() => setIsOpen(!isOpen)}>
      <SidebarMenuSubItem>
        <CollapsibleTrigger asChild>
          <SidebarMenuSubButton className="text-muted-foreground hover:text-sidebar-foreground">
            {isOpen ? (
              <ChevronDown className="h-3 w-3" />
            ) : (
              <ChevronRight className="h-3 w-3" />
            )}
            <Folder className="h-3 w-3" />
            <span className="text-xs">{schema.name}</span>
          </SidebarMenuSubButton>
        </CollapsibleTrigger>
        <CollapsibleContent>
          <SidebarMenuSub>
            {schema.tables.map((table) => (
              <SidebarMenuSubItem key={table.name}>
                <SidebarMenuSubButton
                  onClick={() => handleTableClick(table.name)}
                  className="text-muted-foreground hover:text-sidebar-foreground"
                >
                  <Table className="h-3 w-3" />
                  <span className="text-xs">{table.name}</span>
                </SidebarMenuSubButton>
              </SidebarMenuSubItem>
            ))}
          </SidebarMenuSub>
        </CollapsibleContent>
      </SidebarMenuSubItem>
    </Collapsible>
  );
};
