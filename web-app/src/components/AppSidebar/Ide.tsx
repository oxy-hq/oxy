import { SquareTerminal } from "lucide-react";
import { Link, useLocation } from "react-router-dom";
import {
  SidebarMenuButton,
  SidebarMenuItem,
} from "@/components/ui/shadcn/sidebar";

const Ide = () => {
  const location = useLocation();
  const isIdePage = location.pathname.startsWith("/ide");

  return (
    <SidebarMenuItem>
      <SidebarMenuButton asChild isActive={isIdePage}>
        <Link to="/ide">
          <SquareTerminal />
          <span>IDE</span>
        </Link>
      </SidebarMenuButton>
    </SidebarMenuItem>
  );
};

export default Ide;
