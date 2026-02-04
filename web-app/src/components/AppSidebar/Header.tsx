import { ChevronsLeft, DiamondPlus, Home } from "lucide-react";
import { Link, useLocation } from "react-router-dom";
import { Button } from "@/components/ui/shadcn/button";
import {
  SidebarGroup,
  SidebarHeader,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem
} from "@/components/ui/shadcn/sidebar";
import useSidebar from "@/components/ui/shadcn/sidebar-context";
import { useAuth } from "@/contexts/AuthContext";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import ROUTES from "@/libs/utils/routes";
import useTheme from "@/stores/useTheme";
import Ide from "./Ide";
import Ontology from "./Ontology";
import Workspaces from "./Workspaces";

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
    <SidebarGroup className='gap-4 px-2 pt-2 pb-6'>
      <SidebarHeader className='h-[32px] flex-row items-center justify-between pb-0 pl-1-5'>
        <Link to={homeUri} className='flex min-w-0 items-center gap-2'>
          <img src={theme === "dark" ? "/oxy-dark.svg" : "/oxy-light.svg"} alt='Oxy' />
          <span className='truncate font-medium text-sm'>{project.name}</span>
        </Link>

        {open && (
          <Button onClick={toggleSidebar} variant='ghost' className='m-0 p-0' size='icon'>
            <ChevronsLeft />
          </Button>
        )}
      </SidebarHeader>
      <SidebarMenu>
        <SidebarMenuItem>
          <SidebarMenuButton asChild>
            <Link to={homeUri}>
              <DiamondPlus />
              <span data-testid='start-new-thread'>Start new thread</span>
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
        <Ide />
        <Ontology />
        {authConfig.cloud && <Workspaces />}
      </SidebarMenu>
    </SidebarGroup>
  );
}
