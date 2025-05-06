import {
  Sidebar as ShadcnSidebar,
  SidebarContent,
  SidebarGroup,
  SidebarGroupContent,
  SidebarGroupLabel,
  SidebarMenu,
} from "@/components/ui/shadcn/sidebar";
import Threads from "./Threads";
import { Workflows } from "./Workflows";
import Ide from "./Ide";
import { Apps } from "./Apps";
import { Header } from "./Header";
import { Footer } from "./Footer";
import { Tasks } from "./Tasks";

export function AppSidebar() {
  return (
    <ShadcnSidebar className="bg-sidebar-background">
      <Header />

      <SidebarGroup>
        <SidebarGroupLabel>Developer Console</SidebarGroupLabel>
        <SidebarMenu>
          <Ide />
        </SidebarMenu>
      </SidebarGroup>
      <SidebarContent className="customScrollbar flex flex-col">
        <SidebarGroup>
          <SidebarGroupContent>
            <SidebarMenu>
              <Threads />
              <Tasks />
              <Workflows />
              <Apps />
            </SidebarMenu>
          </SidebarGroupContent>
        </SidebarGroup>
      </SidebarContent>
      <Footer />
    </ShadcnSidebar>
  );
}
