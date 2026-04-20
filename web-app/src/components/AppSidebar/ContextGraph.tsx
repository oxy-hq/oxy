import { Network } from "lucide-react";
import { Link, useLocation } from "react-router-dom";
import { SidebarMenuButton, SidebarMenuItem } from "@/components/ui/shadcn/sidebar";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import ROUTES from "@/libs/utils/routes";
import useCurrentOrg from "@/stores/useCurrentOrg";

const ContextGraph = () => {
  const location = useLocation();
  const { project } = useCurrentProjectBranch();
  const orgSlug = useCurrentOrg((s) => s.org?.slug) ?? "";

  const contextGraphUri = ROUTES.ORG(orgSlug).WORKSPACE(project.id).CONTEXT_GRAPH;
  const isContextGraphPage = location.pathname === contextGraphUri;

  return (
    <SidebarMenuItem>
      <SidebarMenuButton asChild isActive={isContextGraphPage}>
        <Link to={contextGraphUri}>
          <Network />
          <span>Context Graph</span>
        </Link>
      </SidebarMenuButton>
    </SidebarMenuItem>
  );
};

export default ContextGraph;
