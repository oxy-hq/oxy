import { Home, DiamondPlus, ChevronsLeft } from "lucide-react";
import { Link, useLocation, useParams } from "react-router-dom";
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
import Organizations from "./Organizations";

export function Header() {
  const location = useLocation();

  const { theme } = useTheme();
  const { toggleSidebar, open } = useSidebar();
  const { projectId } = useParams();
  if (!projectId) {
    return null;
  }
  const homeUri = ROUTES.PROJECT(projectId).HOME;
  const isHome = location.pathname === homeUri;

  return (
    <SidebarGroup className="gap-2">
      <SidebarHeader className="pt-0 pb-0 pl-1-5 h-[32px] flex-row items-center justify-between">
        <div>
          <img
            src={theme === "dark" ? "/oxy-dark.svg" : "/oxy-light.svg"}
            alt="Oxy"
          />
        </div>

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
        <Organizations />
      </SidebarMenu>
    </SidebarGroup>
  );
}
