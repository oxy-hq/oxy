import { Workflow } from "lucide-react";
import { useState } from "react";
import { Link, useLocation } from "react-router-dom";
import { Button } from "@/components/ui/shadcn/button";
import {
  SidebarMenuButton,
  SidebarMenuItem,
  SidebarMenuSub,
  SidebarMenuSubButton,
  SidebarMenuSubItem
} from "@/components/ui/shadcn/sidebar";
import useWorkflows from "@/hooks/api/workflows/useWorkflows";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import ROUTES from "@/libs/utils/routes";
import ItemsSkeleton from "./ItemsSkeleton";

export function Workflows() {
  const [showAll, setShowAll] = useState(false);
  const location = useLocation();
  const { data: workflows, isPending } = useWorkflows();
  const { project } = useCurrentProjectBranch();
  const projectId = project.id;

  const visibleWorkflows = showAll ? workflows : workflows?.slice(0, 5);

  return (
    <SidebarMenuItem>
      <SidebarMenuButton asChild>
        <div>
          <Workflow />
          <span>Automations</span>
        </div>
      </SidebarMenuButton>
      <SidebarMenuSub className='ml-4'>
        {isPending && <ItemsSkeleton />}

        {!isPending &&
          visibleWorkflows?.map((workflow) => {
            const pathb64 = btoa(workflow.path || "");
            const workflowUri = ROUTES.PROJECT(projectId).WORKFLOW(pathb64).ROOT;
            return (
              <SidebarMenuSubItem key={pathb64}>
                <SidebarMenuSubButton asChild isActive={location.pathname === workflowUri}>
                  <Link to={workflowUri} data-testid={`workflow-link-${workflow.name}`}>
                    <span>{workflow.name}</span>
                  </Link>
                </SidebarMenuSubButton>
              </SidebarMenuSubItem>
            );
          })}
        {workflows && workflows.length > 5 && (
          <Button
            size='sm'
            variant='ghost'
            onClick={() => setShowAll(!showAll)}
            className='w-full py-1 text-left text-muted-foreground text-sm hover:text-foreground'
          >
            {showAll ? "Show less" : `Show all (${workflows.length} automations)`}
          </Button>
        )}
      </SidebarMenuSub>
    </SidebarMenuItem>
  );
}
