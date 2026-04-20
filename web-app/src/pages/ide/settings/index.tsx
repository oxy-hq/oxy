import { ChevronsRight, Database, GitBranch, Key, KeyRound } from "lucide-react";
import type React from "react";
import { useState } from "react";
import { Link, Outlet, useLocation } from "react-router-dom";
import { Button } from "@/components/ui/shadcn/button";
import {
  ResizableHandle,
  ResizablePanel,
  ResizablePanelGroup
} from "@/components/ui/shadcn/resizable";
import {
  SidebarContent,
  SidebarGroup,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem
} from "@/components/ui/shadcn/sidebar";
import useCurrentUser from "@/hooks/api/users/useCurrentUser";
import useAuthConfig from "@/hooks/auth/useAuthConfig";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { FEATURES } from "@/libs/features";
import ROUTES from "@/libs/utils/routes";
import { SidebarHeader } from "@/pages/ide/components/SidebarHeader";
import { VersionBadge } from "./VersionBadge";

const SettingsSidebar: React.FC<{
  setSidebarOpen: (open: boolean) => void;
}> = ({ setSidebarOpen }) => {
  const location = useLocation();
  const { project } = useCurrentProjectBranch();
  const projectId = project.id;
  const { data: currentUser } = useCurrentUser();
  const { data: authConfig } = useAuthConfig();
  const isAdmin = currentUser?.is_admin ?? false;
  // Hide API Keys only when auth is disabled (there's no identity to key off).
  // `single_workspace` is orthogonal to auth — a single-workspace deployment
  // with auth enabled still needs API keys for programmatic access.
  const isLocal = !authConfig?.auth_enabled;

  return (
    <div className='flex h-full flex-col overflow-hidden bg-sidebar-background'>
      <SidebarHeader title='Settings' onCollapse={() => setSidebarOpen(false)} />
      <SidebarContent className='h-full flex-1 overflow-y-auto'>
        <SidebarGroup className='px-1 pt-2'>
          <SidebarMenu>
            <SidebarMenuItem>
              <SidebarMenuButton
                asChild
                isActive={location.pathname === ROUTES.WORKSPACE(projectId).IDE.SETTINGS.DATABASES}
              >
                <Link to={ROUTES.WORKSPACE(projectId).IDE.SETTINGS.DATABASES}>
                  <Database className='h-4 w-4' />
                  <span>Databases</span>
                </Link>
              </SidebarMenuButton>
            </SidebarMenuItem>
            {FEATURES.LINKED_REPOS && (
              <SidebarMenuItem>
                <SidebarMenuButton
                  asChild
                  isActive={
                    location.pathname === ROUTES.WORKSPACE(projectId).IDE.SETTINGS.REPOSITORIES
                  }
                >
                  <Link to={ROUTES.WORKSPACE(projectId).IDE.SETTINGS.REPOSITORIES}>
                    <GitBranch className='h-4 w-4' />
                    <span>Repositories</span>
                  </Link>
                </SidebarMenuButton>
              </SidebarMenuItem>
            )}
            {!isLocal && (
              <SidebarMenuItem>
                <SidebarMenuButton
                  asChild
                  isActive={location.pathname === ROUTES.WORKSPACE(projectId).IDE.SETTINGS.API_KEYS}
                >
                  <Link to={ROUTES.WORKSPACE(projectId).IDE.SETTINGS.API_KEYS}>
                    <Key className='h-4 w-4' />
                    <span>API Keys</span>
                  </Link>
                </SidebarMenuButton>
              </SidebarMenuItem>
            )}
            {isAdmin && (
              <SidebarMenuItem>
                <SidebarMenuButton
                  asChild
                  isActive={location.pathname === ROUTES.WORKSPACE(projectId).IDE.SETTINGS.SECRETS}
                >
                  <Link to={ROUTES.WORKSPACE(projectId).IDE.SETTINGS.SECRETS}>
                    <KeyRound className='h-4 w-4' />
                    <span>Secrets</span>
                  </Link>
                </SidebarMenuButton>
              </SidebarMenuItem>
            )}
          </SidebarMenu>
        </SidebarGroup>
      </SidebarContent>
      <div className='flex items-center px-3 py-2'>
        <VersionBadge />
      </div>
    </div>
  );
};

const SettingsLayout: React.FC = () => {
  const [sidebarOpen, setSidebarOpen] = useState(true);

  return (
    <ResizablePanelGroup direction='horizontal' className='flex-1'>
      {sidebarOpen ? (
        <>
          <ResizablePanel defaultSize={20} minSize={10} className='min-w-[200px]'>
            <SettingsSidebar setSidebarOpen={setSidebarOpen} />
          </ResizablePanel>
          <ResizableHandle withHandle />
        </>
      ) : (
        <div className='flex items-start border-r bg-sidebar-background px-1 py-2'>
          <Button
            variant='ghost'
            size='icon'
            onClick={() => setSidebarOpen(true)}
            tooltip={{ content: "Expand Sidebar", side: "right" }}
            className='h-8 w-8'
          >
            <ChevronsRight className='h-4 w-4' />
          </Button>
        </div>
      )}
      <ResizablePanel defaultSize={sidebarOpen ? 80 : 100} minSize={20}>
        <Outlet />
      </ResizablePanel>
    </ResizablePanelGroup>
  );
};

export default SettingsLayout;
