import { ChevronsRight, HeartPulse, Inbox, List, Radio } from "lucide-react";
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
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import ROUTES from "@/libs/utils/routes";
import { SidebarHeader } from "@/pages/ide/components/SidebarHeader";
import useCurrentOrg from "@/stores/useCurrentOrg";

const CoordinatorSidebar: React.FC<{
  setSidebarOpen: (open: boolean) => void;
}> = ({ setSidebarOpen }) => {
  const location = useLocation();
  const { project } = useCurrentProjectBranch();
  const projectId = project.id;
  const orgSlug = useCurrentOrg((s) => s.org?.slug) ?? "";
  const ws = ROUTES.ORG(orgSlug).WORKSPACE(projectId);

  return (
    <div className='flex h-full flex-col overflow-hidden bg-sidebar-background'>
      <SidebarHeader title='Coordinator' onCollapse={() => setSidebarOpen(false)} />
      <SidebarContent className='h-full flex-1 overflow-y-auto'>
        <SidebarGroup className='px-1 pt-2'>
          <SidebarMenu>
            <SidebarMenuItem>
              <SidebarMenuButton
                asChild
                isActive={location.pathname.includes("/coordinator/active-runs")}
              >
                <Link to={ws.IDE.COORDINATOR.ACTIVE_RUNS}>
                  <Radio className='h-4 w-4' />
                  <span>Active Runs</span>
                </Link>
              </SidebarMenuButton>
            </SidebarMenuItem>
            <SidebarMenuItem>
              <SidebarMenuButton
                asChild
                isActive={location.pathname.includes("/coordinator/run-history")}
              >
                <Link to={ws.IDE.COORDINATOR.RUN_HISTORY}>
                  <List className='h-4 w-4' />
                  <span>Run History</span>
                </Link>
              </SidebarMenuButton>
            </SidebarMenuItem>
            <SidebarMenuItem>
              <SidebarMenuButton
                asChild
                isActive={location.pathname.includes("/coordinator/recovery")}
              >
                <Link to={ws.IDE.COORDINATOR.RECOVERY}>
                  <HeartPulse className='h-4 w-4' />
                  <span>Recovery</span>
                </Link>
              </SidebarMenuButton>
            </SidebarMenuItem>
            <SidebarMenuItem>
              <SidebarMenuButton
                asChild
                isActive={location.pathname.includes("/coordinator/queue")}
              >
                <Link to={ws.IDE.COORDINATOR.QUEUE}>
                  <Inbox className='h-4 w-4' />
                  <span>Queue Health</span>
                </Link>
              </SidebarMenuButton>
            </SidebarMenuItem>
          </SidebarMenu>
        </SidebarGroup>
      </SidebarContent>
    </div>
  );
};

const CoordinatorLayout: React.FC = () => {
  const [sidebarOpen, setSidebarOpen] = useState(true);

  return (
    <ResizablePanelGroup direction='horizontal' className='flex-1'>
      {sidebarOpen ? (
        <>
          <ResizablePanel defaultSize={20} minSize={10} className='min-w-[200px]'>
            <CoordinatorSidebar setSidebarOpen={setSidebarOpen} />
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

export default CoordinatorLayout;
