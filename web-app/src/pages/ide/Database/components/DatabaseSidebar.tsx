import { Database as DatabaseIcon, Plus, RotateCw } from "lucide-react";
import type React from "react";
import { Link } from "react-router-dom";
import { Button } from "@/components/ui/shadcn/button";
import { SidebarContent, SidebarGroup, SidebarMenu } from "@/components/ui/shadcn/sidebar";
import { Spinner } from "@/components/ui/shadcn/spinner";
import useDatabases from "@/hooks/api/databases/useDatabases";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import ROUTES from "@/libs/utils/routes";
import { SidebarHeader } from "@/pages/ide/components/SidebarHeader";
import useCurrentOrg from "@/stores/useCurrentOrg";
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
  const orgSlug = useCurrentOrg((s) => s.org?.slug) ?? "";

  const { data: databases = [], isLoading, refetch, isFetching } = useDatabases();

  return (
    <div className='flex h-full flex-col overflow-hidden bg-sidebar-background'>
      <SidebarHeader
        title='Connections'
        onCollapse={() => setSidebarOpen(!sidebarOpen)}
        actions={
          <>
            <Link to={ROUTES.ORG(orgSlug).WORKSPACE(projectId).IDE.SETTINGS.DATABASES}>
              <Button tooltip='Add new connection' variant='ghost' size='sm'>
                <Plus />
              </Button>
            </Link>
            <Button
              tooltip='Refresh'
              variant='ghost'
              size='sm'
              onClick={() => refetch()}
              disabled={isFetching}
            >
              <RotateCw className={` ${isFetching ? "animate-spin" : ""}`} />
            </Button>
          </>
        }
      />
      <SidebarContent className='h-full flex-1'>
        <SidebarGroup className='px-1 pt-2'>
          {isLoading && (
            <div className='flex items-center justify-center p-4'>
              <Spinner />
            </div>
          )}

          {!isLoading && databases.length === 0 && (
            <div className='flex flex-col items-center justify-center p-4 text-muted-foreground text-sm'>
              <DatabaseIcon className='mb-2 h-8 w-8 opacity-50' />
              <p>No databases configured</p>
              <Link
                to={ROUTES.ORG(orgSlug).WORKSPACE(projectId).IDE.SETTINGS.DATABASES}
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
                  <ConnectionItem key={database.name} database={database} />
                ))}
            </SidebarMenu>
          )}
        </SidebarGroup>
      </SidebarContent>
    </div>
  );
};
