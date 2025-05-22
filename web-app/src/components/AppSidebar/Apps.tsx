import { LayoutDashboard } from "lucide-react";
import { Link, useLocation } from "react-router-dom";
import {
  SidebarMenuButton,
  SidebarMenuItem,
  SidebarMenuSub,
  SidebarMenuSubButton,
  SidebarMenuSubItem,
} from "@/components/ui/shadcn/sidebar";
import useApps from "@/hooks/api/useApps";
import ItemsSkeleton from "./ItemsSkeleton";

export function Apps() {
  const location = useLocation();
  const { data: apps, isPending } = useApps();

  return (
    <SidebarMenuItem>
      <SidebarMenuButton asChild>
        <div>
          <LayoutDashboard />
          <span>Apps</span>
        </div>
      </SidebarMenuButton>
      <SidebarMenuSub>
        {isPending && <ItemsSkeleton />}

        {!isPending &&
          apps?.map((app) => {
            const pathb64 = btoa(app.path);
            const appUri = `/apps/${pathb64}`;
            return (
              <SidebarMenuSubItem key={pathb64}>
                <SidebarMenuSubButton
                  asChild
                  isActive={location.pathname === appUri}
                >
                  <Link to={appUri}>
                    <span>{app.name}</span>
                  </Link>
                </SidebarMenuSubButton>
              </SidebarMenuSubItem>
            );
          })}
      </SidebarMenuSub>
    </SidebarMenuItem>
  );
}
