import { LayoutDashboard } from "lucide-react";
import { Link, useLocation } from "react-router-dom";
import {
  SidebarMenuButton,
  SidebarMenuItem,
  SidebarMenuSub,
  SidebarMenuSubButton,
  SidebarMenuSubItem,
} from "@/components/ui/shadcn/sidebar";
import useApps from "@/hooks/api/apps/useApps";
import ItemsSkeleton from "./ItemsSkeleton";
import ROUTES from "@/libs/utils/routes";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";

export function Apps() {
  const location = useLocation();
  const { data: apps, isPending } = useApps();
  const { project } = useCurrentProjectBranch();
  const projectId = project.id;

  return (
    <SidebarMenuItem>
      <SidebarMenuButton asChild>
        <div>
          <LayoutDashboard />
          <span>Apps</span>
        </div>
      </SidebarMenuButton>
      <SidebarMenuSub className="ml-4">
        {isPending && <ItemsSkeleton />}

        {!isPending &&
          apps?.map((app) => {
            const pathb64 = btoa(app.path);
            const appUri = ROUTES.PROJECT(projectId).APP(pathb64);
            return (
              <SidebarMenuSubItem key={pathb64}>
                <SidebarMenuSubButton
                  asChild
                  isActive={location.pathname === appUri}
                >
                  <Link to={appUri} data-testid={`app-link-${app.name}`}>
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
