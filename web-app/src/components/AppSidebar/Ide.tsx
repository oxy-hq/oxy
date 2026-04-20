import { SquareTerminal } from "lucide-react";
import { Link, useLocation } from "react-router-dom";
import { SidebarMenuButton, SidebarMenuItem } from "@/components/ui/shadcn/sidebar";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import ROUTES from "@/libs/utils/routes";
import useCurrentOrg from "@/stores/useCurrentOrg";

const Ide = () => {
  const location = useLocation();
  const { project } = useCurrentProjectBranch();
  const orgSlug = useCurrentOrg((s) => s.org?.slug) ?? "";

  const ideUri = ROUTES.ORG(orgSlug).WORKSPACE(project.id).IDE.ROOT;
  const isIdePage = location.pathname.startsWith(ideUri);

  return (
    <SidebarMenuItem>
      <SidebarMenuButton asChild isActive={isIdePage}>
        <Link to={ideUri}>
          <SquareTerminal />
          <span>Developer Portal</span>
        </Link>
      </SidebarMenuButton>
    </SidebarMenuItem>
  );
};

export default Ide;
