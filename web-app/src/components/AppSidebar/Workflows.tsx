import { Workflow } from "lucide-react";
import { Link, useLocation } from "react-router-dom";
import {
  SidebarMenuButton,
  SidebarMenuItem,
  SidebarMenuSub,
  SidebarMenuSubButton,
  SidebarMenuSubItem,
} from "@/components/ui/shadcn/sidebar";
import useWorkflows from "@/hooks/api/useWorkflows";

export function Workflows() {
  const location = useLocation();
  const { data: workflows } = useWorkflows();

  return (
    <SidebarMenuItem>
      <SidebarMenuButton asChild>
        <div>
          <Workflow />
          <span>Workflows</span>
        </div>
      </SidebarMenuButton>
      <SidebarMenuSub>
        {workflows?.map((workflow) => {
          const pathb64 = btoa(workflow.path);
          const workflowUri = `/workflows/${pathb64}`;
          return (
            <SidebarMenuSubItem>
              <SidebarMenuSubButton
                asChild
                isActive={location.pathname === workflowUri}
              >
                <Link to={workflowUri}>
                  <span>{workflow.name}</span>
                </Link>
              </SidebarMenuSubButton>
            </SidebarMenuSubItem>
          );
        })}
      </SidebarMenuSub>
    </SidebarMenuItem>
  );
}
