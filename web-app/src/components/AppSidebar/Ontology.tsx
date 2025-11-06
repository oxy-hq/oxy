import { Network } from "lucide-react";
import { Link, useLocation } from "react-router-dom";
import {
  SidebarMenuButton,
  SidebarMenuItem,
} from "@/components/ui/shadcn/sidebar";
import ROUTES from "@/libs/utils/routes";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";

const Ontology = () => {
  const location = useLocation();
  const { project } = useCurrentProjectBranch();

  const ontologyUri = ROUTES.PROJECT(project.id).ONTOLOGY;
  const isOntologyPage = location.pathname === ontologyUri;

  return (
    <SidebarMenuItem>
      <SidebarMenuButton asChild isActive={isOntologyPage}>
        <Link to={ontologyUri}>
          <Network />
          <span>Ontology</span>
        </Link>
      </SidebarMenuButton>
    </SidebarMenuItem>
  );
};

export default Ontology;
