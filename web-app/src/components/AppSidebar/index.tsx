import { Home, MessageSquarePlus } from "lucide-react";
import { Link, useLocation } from "react-router-dom";
import {
  Sidebar as ShadcnSidebar,
  SidebarContent,
  SidebarGroup,
  SidebarGroupContent,
  SidebarHeader,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
} from "@/components/ui/shadcn/sidebar";
import Threads from "./Threads";
import { Workflows } from "./Workflows";

export function AppSidebar() {
  const location = useLocation();
  const isHome = location.pathname === "/";
  const isNew = location.pathname === "/new";

  return (
    <ShadcnSidebar className="p-2">
      <SidebarHeader>
        <div className="p-2">
          <img src="/oxy-logo.svg" alt="Oxy" />
        </div>
      </SidebarHeader>
      <SidebarContent className="customScrollbar">
        <SidebarGroup>
          <SidebarGroupContent>
            <SidebarMenu>
              <SidebarMenuItem>
                <SidebarMenuButton asChild isActive={isNew}>
                  <Link to="/new">
                    <MessageSquarePlus />
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
              <Threads />
              <Workflows />
            </SidebarMenu>
          </SidebarGroupContent>
        </SidebarGroup>
      </SidebarContent>
    </ShadcnSidebar>
  );
}
