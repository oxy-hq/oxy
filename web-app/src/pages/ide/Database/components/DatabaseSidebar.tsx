import { Database as DatabaseIcon, Loader2, Plus } from "lucide-react";
import type React from "react";
import { Link } from "react-router-dom";
import { Button } from "@/components/ui/shadcn/button";
import { SidebarContent, SidebarGroup, SidebarMenu } from "@/components/ui/shadcn/sidebar";
import useDatabases from "@/hooks/api/databases/useDatabases";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import ROUTES from "@/libs/utils/routes";
import { SidebarHeader } from "@/pages/ide/components/SidebarHeader";
import useDatabaseClient from "@/stores/useDatabaseClient";
import { ConnectionItem } from "./ConnectionItem";

interface DatabaseSidebarProps {
  sidebarOpen: boolean;
  setSidebarOpen: (open: boolean) => void;
}

export const DatabaseSidebar: React.FC<DatabaseSidebarProps> = ({
  sidebarOpen,
  setSidebarOpen
}) => {
  const { project } = useCurrentProjectBranch();
  const projectId = project.id;

  const { data: databases = [], isLoading } = useDatabases();
  const { activeConnectionId, setActiveConnection } = useDatabaseClient();

  return (
    <div className='flex h-full flex-col overflow-hidden bg-sidebar-background'>
      <SidebarHeader
        title='Connections'
        onCollapse={() => setSidebarOpen(!sidebarOpen)}
        actions={
          <Link to={ROUTES.PROJECT(projectId).IDE.SETTINGS.DATABASES}>
            <Button tooltip='Add new connection' variant='ghost' size='icon' className='h-6 w-6'>
              <Plus className='h-4 w-4' />
            </Button>
          </Link>
        }
      />
      <SidebarContent className='customScrollbar h-full flex-1 overflow-y-auto'>
        <SidebarGroup className='pt-2'>
          {isLoading && (
            <div className='flex items-center justify-center p-4'>
              <Loader2 className='h-4 w-4 animate-spin' />
            </div>
          )}

          {!isLoading && databases.length === 0 && (
            <div className='flex flex-col items-center justify-center p-4 text-muted-foreground text-sm'>
              <DatabaseIcon className='mb-2 h-8 w-8 opacity-50' />
              <p>No databases configured</p>
              <Link
                to={ROUTES.PROJECT(projectId).IDE.SETTINGS.DATABASES}
                className='mt-1 text-primary text-xs hover:underline'
              >
                Add database connection
              </Link>
            </div>
          )}

          {!isLoading && databases.length > 0 && (
            <SidebarMenu className='pb-20'>
              {databases
                .sort((a, b) => a.name.localeCompare(b.name))
                .map((database) => (
                  <ConnectionItem
                    key={database.name}
                    database={database}
                    isActive={activeConnectionId === database.name.toLowerCase()}
                    onSelect={() => setActiveConnection(database.name.toLowerCase())}
                  />
                ))}
            </SidebarMenu>
          )}
        </SidebarGroup>
      </SidebarContent>
    </div>
  );
};
