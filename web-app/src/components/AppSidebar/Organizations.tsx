import { Building2 } from "lucide-react";
import { Link, useLocation } from "react-router-dom";
import {
  SidebarMenuButton,
  SidebarMenuItem,
} from "@/components/ui/shadcn/sidebar";
import ROUTES from "@/libs/utils/routes";

export default function Organizations() {
  const location = useLocation();
  const isOrganizationsPage = location.pathname.startsWith(ROUTES.ORG.ROOT);

  return (
    <SidebarMenuItem>
      <SidebarMenuButton asChild isActive={isOrganizationsPage}>
        <Link to={ROUTES.ORG.ROOT}>
          <Building2 />
          <span>Organizations</span>
        </Link>
      </SidebarMenuButton>
    </SidebarMenuItem>
  );
}
