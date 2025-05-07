import { Home, DiamondPlus, ChevronsLeft } from "lucide-react";
import { Link, useLocation } from "react-router-dom";
import {
  SidebarGroup,
  SidebarHeader,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
  useSidebar,
} from "@/components/ui/shadcn/sidebar";
import useTheme from "@/stores/useTheme";
import { Button } from "@/components/ui/shadcn/button";

export function Header() {
  const location = useLocation();
  const isHome = location.pathname === "/";
  const isNew = location.pathname === "/new";
  const { theme } = useTheme();
  const { toggleSidebar, open } = useSidebar();

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
          <SidebarMenuButton asChild isActive={isNew}>
            <Link to="/new">
              <DiamondPlus />
              <span>Start new thread</span>
            </Link>
          </SidebarMenuButton>
        </SidebarMenuItem>
        <SidebarMenuItem>
          <SidebarMenuButton asChild isActive={isHome}>
            <Link to="/">
              <Home />
              <span>Home</span>
            </Link>
          </SidebarMenuButton>
        </SidebarMenuItem>
      </SidebarMenu>
    </SidebarGroup>
  );
}
