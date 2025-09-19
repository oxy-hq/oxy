import { SquareTerminal } from "lucide-react";
import { Link, useLocation, useParams } from "react-router-dom";
import {
  SidebarMenuButton,
  SidebarMenuItem,
} from "@/components/ui/shadcn/sidebar";
import ROUTES from "@/libs/utils/routes";

const Ide = () => {
  const location = useLocation();
  const { projectId } = useParams();
  
  if (!projectId) {
    throw new Error("Project ID is required");
  }

  const ideUri = ROUTES.PROJECT(projectId).IDE.ROOT;
  const isIdePage = location.pathname.startsWith(ideUri);

  return (
    <SidebarMenuItem>
      <SidebarMenuButton asChild isActive={isIdePage}>
        <Link to={ideUri}>
          <SquareTerminal />
          <span>IDE</span>
        </Link>
      </SidebarMenuButton>
    </SidebarMenuItem>
  );
};

export default Ide;
