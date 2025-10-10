import { Home, DiamondPlus, ChevronsLeft } from "lucide-react";
import { Link, useLocation } from "react-router-dom";
import {
  SidebarGroup,
  SidebarHeader,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
} from "@/components/ui/shadcn/sidebar";
import useSidebar from "@/components/ui/shadcn/sidebar-context";
import useTheme from "@/stores/useTheme";
import { Button } from "@/components/ui/shadcn/button";
import ROUTES from "@/libs/utils/routes";
import Workspaces from "./Workspaces";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { useAuth } from "@/contexts/AuthContext";

export function Header() {
  const location = useLocation();

  const { authConfig } = useAuth();

  const { theme } = useTheme();
  const { toggleSidebar, open } = useSidebar();
  const { project } = useCurrentProjectBranch();
  const projectId = project.id;
  const homeUri = ROUTES.PROJECT(projectId).HOME;
  const isHome = location.pathname === homeUri;

  return (
    <SidebarGroup className="gap-4">
      <SidebarHeader className="pt-0 pb-0 pl-1-5 h-[32px] flex-row items-center justify-between">
        <Link to={homeUri} className="flex gap-2 items-center min-w-0">
          <img
            src={theme === "dark" ? "/oxy-dark.svg" : "/oxy-light.svg"}
            alt="Oxy"
          />
          <span className="text-sm font-medium truncate">{project.name}</span>
        </Link>

        {open && (
          <Button
            onClick={toggleSidebar}
            variant="ghost"
            className="p-0 m-0"
            size="icon"
          >
            <ChevronsLeft />
          </Button>
        )}
      </SidebarHeader>
      <SidebarMenu>
        <SidebarMenuItem>
          <SidebarMenuButton asChild>
            <Link to={homeUri}>
              <DiamondPlus />
              <span>Start new thread</span>
            </Link>
          </SidebarMenuButton>
        </SidebarMenuItem>
        <SidebarMenuItem>
          <SidebarMenuButton asChild isActive={isHome}>
            <Link to={homeUri}>
              <Home />
              <span>Home</span>
            </Link>
          </SidebarMenuButton>
        </SidebarMenuItem>
        {authConfig.cloud && <Workspaces />}
      </SidebarMenu>
    </SidebarGroup>
  );
}
