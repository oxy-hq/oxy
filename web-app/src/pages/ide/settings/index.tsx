import React, { useState } from "react";
import { Link, useLocation, Outlet } from "react-router-dom";
import {
  ResizableHandle,
  ResizablePanel,
  ResizablePanelGroup,
} from "@/components/ui/shadcn/resizable";
import { ChevronsRight, Database, FileText } from "lucide-react";
import {
  SidebarContent,
  SidebarGroup,
  SidebarMenu,
  SidebarMenuItem,
  SidebarMenuSubButton,
} from "@/components/ui/shadcn/sidebar";
import { Button } from "@/components/ui/shadcn/button";
import { SidebarHeader } from "@/pages/ide/components/SidebarHeader";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import ROUTES from "@/libs/utils/routes";

const SettingsSidebar: React.FC<{
  setSidebarOpen: (open: boolean) => void;
}> = ({ setSidebarOpen }) => {
  const location = useLocation();
  const { project } = useCurrentProjectBranch();
  const projectId = project.id;

  return (
    <div className="flex h-full flex-col overflow-hidden bg-sidebar-background">
      <SidebarHeader
        title="Settings"
        onCollapse={() => setSidebarOpen(false)}
      />
      <SidebarContent className="customScrollbar h-full flex-1 overflow-y-auto">
        <SidebarGroup className="pt-2">
          <SidebarMenu>
            <SidebarMenuItem>
              <SidebarMenuSubButton
                asChild
                isActive={
                  location.pathname ===
                  ROUTES.PROJECT(projectId).IDE.SETTINGS.DATABASES
                }
              >
                <Link to={ROUTES.PROJECT(projectId).IDE.SETTINGS.DATABASES}>
                  <Database className="h-4 w-4" />
                  <span>Databases</span>
                </Link>
              </SidebarMenuSubButton>
            </SidebarMenuItem>
            <SidebarMenuItem>
              <SidebarMenuSubButton
                asChild
                isActive={
                  location.pathname ===
                  ROUTES.PROJECT(projectId).IDE.SETTINGS.ACTIVITY_LOGS
                }
              >
                <Link to={ROUTES.PROJECT(projectId).IDE.SETTINGS.ACTIVITY_LOGS}>
                  <FileText className="h-4 w-4" />
                  <span>Activity Logs</span>
                </Link>
              </SidebarMenuSubButton>
            </SidebarMenuItem>
          </SidebarMenu>
        </SidebarGroup>
      </SidebarContent>
    </div>
  );
};

const SettingsLayout: React.FC = () => {
  const [sidebarOpen, setSidebarOpen] = useState(true);

  return (
    <ResizablePanelGroup direction="horizontal" className="flex-1">
      {sidebarOpen ? (
        <>
          <ResizablePanel
            defaultSize={20}
            minSize={10}
            className="min-w-[200px]"
          >
            <SettingsSidebar setSidebarOpen={setSidebarOpen} />
          </ResizablePanel>
          <ResizableHandle />
        </>
      ) : (
        <div className="border-r bg-sidebar-background flex items-start py-2 px-1">
          <Button
            variant="ghost"
            size="icon"
            onClick={() => setSidebarOpen(true)}
            tooltip={{ content: "Expand Sidebar", side: "right" }}
            className="h-8 w-8"
          >
            <ChevronsRight className="h-4 w-4" />
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
