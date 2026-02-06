import { ChevronDown, ChevronRight, Folder, Table } from "lucide-react";
import React from "react";
import { useNavigate } from "react-router-dom";
import { toast } from "sonner";
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger
} from "@/components/ui/shadcn/collapsible";
import {
  SidebarMenuSub,
  SidebarMenuSubButton,
  SidebarMenuSubItem
} from "@/components/ui/shadcn/sidebar";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import ROUTES from "@/libs/utils/routes";
import useDatabaseClient from "@/stores/useDatabaseClient";
import type { SemanticModels } from "@/types/database";

interface SchemaTreeItemProps {
  schemaName: string;
  dialect?: string;
  databaseName?: string;
  tables: Record<string, SemanticModels>;
}

const getTableNameFromSemanticInfo = (
  semanticInfo: SemanticModels | undefined,
  fallbackName: string
): string => {
  if (!semanticInfo) return fallbackName;
  return semanticInfo.table ?? fallbackName;
};

const generateSelectQuery = (
  tableName: string,
  dialect?: string,
  semanticInfo?: SemanticModels
): string => {
  const normalizedDialect = dialect?.toLowerCase() || "";

  const tableNameFromSemantic = getTableNameFromSemanticInfo(semanticInfo, tableName);

  switch (normalizedDialect) {
    case "bigquery":
      return `SELECT * FROM ${tableNameFromSemantic}\nLIMIT 100;`;
    case "mysql":
      return `SELECT * FROM \`${tableNameFromSemantic}\`\nLIMIT 100;`;
    case "postgres":
    case "postgresql":
    case "redshift":
      return `SELECT * FROM "${tableNameFromSemantic}"\nLIMIT 100;`;
    case "snowflake":
      return `SELECT * FROM "${tableNameFromSemantic}"\nLIMIT 100;`;
    case "clickhouse":
      return `SELECT * FROM ${tableNameFromSemantic}\nLIMIT 100;`;
    case "duckdb": {
      return `SELECT * FROM ${tableNameFromSemantic}\nLIMIT 100;`;
    }
    case "domo":
      return `SELECT * FROM ${tableNameFromSemantic}\nLIMIT 100`;
    default:
      return `SELECT * FROM ${tableNameFromSemantic}\nLIMIT 100;`;
  }
};

export const SchemaTreeItem: React.FC<SchemaTreeItemProps> = ({
  schemaName,
  dialect,
  databaseName,
  tables
}) => {
  const [isOpen, setIsOpen] = React.useState(false);
  const { addTab } = useDatabaseClient();
  const navigate = useNavigate();
  const { project } = useCurrentProjectBranch();
  const projectId = project.id;

  const handleTableClick = (tableName: string, semanticInfo: SemanticModels) => {
    const result = addTab({
      name: `${tableName}.sql`,
      content: generateSelectQuery(tableName, dialect, semanticInfo),
      isDirty: true,
      selectedDatabase: databaseName
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
          <SidebarMenuSubButton className='text-muted-foreground hover:text-sidebar-foreground'>
            {isOpen ? <ChevronDown className='h-3 w-3' /> : <ChevronRight className='h-3 w-3' />}
            <Folder className='h-3 w-3' />
            <span className='text-xs'>{schemaName}</span>
          </SidebarMenuSubButton>
        </CollapsibleTrigger>
        <CollapsibleContent>
          <SidebarMenuSub className='ml-4'>
            {Object.entries(tables).map(([tableName, semanticInfo]) => (
              <SidebarMenuSubItem key={tableName}>
                <SidebarMenuSubButton
                  onClick={() => handleTableClick(tableName, semanticInfo)}
                  className='text-muted-foreground hover:text-sidebar-foreground'
                >
                  <Table className='h-3 w-3' />
                  <span className='text-xs'>{tableName}</span>
                </SidebarMenuSubButton>
              </SidebarMenuSubItem>
            ))}
          </SidebarMenuSub>
        </CollapsibleContent>
      </SidebarMenuSubItem>
    </Collapsible>
  );
};
