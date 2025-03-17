import {
  Home,
  MessageSquarePlus,
  MessagesSquare,
  Workflow,
} from "lucide-react";
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
import useThreads from "@/hooks/api/useThreads";
import useWorkflows from "@/hooks/api/useWorkflows";

export function AppSidebar() {
  const location = useLocation();

  const { data: threads } = useThreads();
  const isHome = location.pathname === "/";
  const isThreads = location.pathname === "/threads";
  const isNew = location.pathname === "/new";
  const { data: workflows } = useWorkflows();
  return (
    <ShadcnSidebar className="p-2">
      <SidebarHeader>
        <div className="p-2">
          <img src="/oxy-logo.svg" alt="Oxy" />
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
                  {threads?.map((thread) => (
                    <SidebarMenuSubItem key={thread.id}>
                      <SidebarMenuSubButton
                        asChild
                        isActive={location.pathname === `/threads/${thread.id}`}
                      >
                        <Link to={`/threads/${thread.id}`}>
                          <span>{thread.title}</span>
                        </Link>
                      </SidebarMenuSubButton>
                    </SidebarMenuSubItem>
                  ))}
                </SidebarMenuSub>
              </SidebarMenuItem>
              <SidebarMenuItem>
                <SidebarMenuButton asChild isActive={isThreads}>
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
            </SidebarMenu>
          </SidebarGroupContent>
        </SidebarGroup>
      </SidebarContent>
    </ShadcnSidebar>
  );
}
