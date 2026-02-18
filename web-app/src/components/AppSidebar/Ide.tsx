import { SquareTerminal } from "lucide-react";
import { Link, useLocation } from "react-router-dom";
import { SidebarMenuButton, SidebarMenuItem } from "@/components/ui/shadcn/sidebar";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import ROUTES from "@/libs/utils/routes";

const Ide = () => {
  const location = useLocation();
  const { project } = useCurrentProjectBranch();

  const ideUri = ROUTES.PROJECT(project.id).IDE.ROOT;
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
