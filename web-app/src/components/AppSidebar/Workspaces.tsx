import { Building2 } from "lucide-react";
import { Link, useLocation } from "react-router-dom";
import { SidebarMenuButton, SidebarMenuItem } from "@/components/ui/shadcn/sidebar";
import ROUTES from "@/libs/utils/routes";

export default function Workspaces() {
  const location = useLocation();
  const isWorkspacesPage = location.pathname.startsWith(ROUTES.WORKSPACE.ROOT);

  return (
    <SidebarMenuItem>
      <SidebarMenuButton asChild isActive={isWorkspacesPage}>
        <Link to={ROUTES.WORKSPACE.ROOT}>
          <Building2 />
          <span>Workspaces</span>
        </Link>
      </SidebarMenuButton>
    </SidebarMenuItem>
  );
}
