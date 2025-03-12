import { Home, MessageSquarePlus, MessagesSquare, Workflow } from "lucide-react";
import { Link, useLocation } from "react-router-dom";
import {
  Sidebar as ShadcnSidebar,
  SidebarContent,
  SidebarGroup,
  SidebarGroupContent,
  SidebarHeader,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
  SidebarMenuSub,
  SidebarMenuSubButton,
  SidebarMenuSubItem,
} from "@/components/ui/shadcn/sidebar";
import useWorkflows from "@/hooks/api/useWorkflows";
import Spinner from "@/components/ui/Spinner";

export function AppSidebar() {
  const location = useLocation();
  const isHome = location.pathname === "/";
  const isThreads = location.pathname === "/threads";
  const isNew = location.pathname === "/new";
  const { data: workflows, isPending: isWorkflowsLoading } = useWorkflows();
  return (
    <ShadcnSidebar className="p-2">
      <SidebarHeader>
        <div className="p-2">
          <img src="/onyx-logo.svg" alt="Onyx" />
        </div>
      </SidebarHeader>
      <SidebarContent>
        <SidebarGroup>
          <SidebarGroupContent>
            <SidebarMenu>
              <SidebarMenuItem>
                <SidebarMenuButton asChild isActive={isNew}>
                  <Link to="/new">
                    <MessageSquarePlus />
                    <span>Start new thread</span>
                  </Link>
                </SidebarMenuButton>
              </SidebarMenuItem>
              <SidebarMenuItem>
                <SidebarMenuButton asChild isActive={isHome}>
                  <Link to="/">
                    <Home />
                    <span>Home</span>
                  </Link>
                </SidebarMenuButton>
              </SidebarMenuItem>
              <SidebarMenuItem>
                <SidebarMenuButton asChild isActive={isThreads}>
                  <Link to="/threads">
                    <MessagesSquare />
                    <span>Threads</span>
                  </Link>
                </SidebarMenuButton>
                <SidebarMenuSub>
                  <SidebarMenuSubItem>
                    <SidebarMenuSubButton
                      asChild
                      isActive={location.pathname === "/threads/1"}
                    >
                      <Link to="/threads/1">
                        <span>Thread 1</span>
                      </Link>
                    </SidebarMenuSubButton>
                  </SidebarMenuSubItem>
                  <SidebarMenuSubItem>
                    <SidebarMenuSubButton
                      asChild
                      isActive={location.pathname === "/threads/2"}
                    >
                      <Link to="/threads/2">
                        <span>Thread 2</span>
                      </Link>
                    </SidebarMenuSubButton>
                  </SidebarMenuSubItem>
                </SidebarMenuSub>
              </SidebarMenuItem>
              <SidebarMenuItem>
                <SidebarMenuButton asChild isActive={isThreads}>
                  <Link to="/workflows">
                    <Workflow />
                    <span>Workflows</span>
                  </Link>
                </SidebarMenuButton>
                {isWorkflowsLoading ? <SidebarMenuSub>
                  <Spinner/>
                </SidebarMenuSub>: <>
                    <SidebarMenuSub>
                        {workflows?.map(
                            workflow => {
                                const pathb64 = btoa(workflow.path)
                                const workflowUri = `/workflows/${pathb64}`
                                return <SidebarMenuSubItem>
                                    <SidebarMenuSubButton
                                        asChild
                                        isActive={location.pathname === workflowUri}
                                    >
                                        <Link to={workflowUri}>
                                            <span>{workflow.name}</span>
                                        </Link>
                                    </SidebarMenuSubButton>
                                </SidebarMenuSubItem>
                            }
                        )}
                    </SidebarMenuSub>
                </>}
              </SidebarMenuItem>
            </SidebarMenu>
          </SidebarGroupContent>
        </SidebarGroup>
      </SidebarContent>
    </ShadcnSidebar>
  );
}
