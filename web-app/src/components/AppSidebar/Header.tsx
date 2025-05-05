import { Home, DiamondPlus } from "lucide-react";
import { Link, useLocation } from "react-router-dom";
import {
  SidebarGroup,
  SidebarHeader,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
} from "@/components/ui/shadcn/sidebar";
import useTheme from "@/stores/useTheme";

export function Header() {
  const location = useLocation();
  const isHome = location.pathname === "/";
  const isNew = location.pathname === "/new";
  const { theme } = useTheme();

  return (
    <SidebarGroup className="gap-2">
      <SidebarHeader className="pt-0 pb-0 pl-1-5 h-[32px] flex-row items-center">
        <div>
          <img
            src={theme === "dark" ? "/oxy-dark.svg" : "/oxy-light.svg"}
            alt="Oxy"
          />
        </div>
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
