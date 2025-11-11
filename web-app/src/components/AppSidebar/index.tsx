import {
  Sidebar as ShadcnSidebar,
  SidebarGroup,
  SidebarGroupLabel,
  SidebarMenu,
} from "@/components/ui/shadcn/sidebar";
import Threads from "./Threads";
import { Workflows } from "./Workflows";
import Ide from "./Ide";
import { Apps } from "./Apps";
import { Header } from "./Header";
import { Footer } from "./Footer";

export function AppSidebar() {
  return (
    <ShadcnSidebar className="bg-sidebar-background">
      <Header />

      <div className="customScrollbar flex flex-col flex-1 overflow-auto min-h-0 scrollbar-gutter-auto">
        <SidebarGroup>
          <SidebarGroupLabel>Developer Console</SidebarGroupLabel>
          <SidebarMenu>
            <Ide />
          </SidebarMenu>
        </SidebarGroup>
        <SidebarGroup>
          <SidebarGroupLabel>Workspace</SidebarGroupLabel>
          <SidebarMenu>
            <Threads />
            <Workflows />
            <Apps />
          </SidebarMenu>
        </SidebarGroup>
      </div>
      <Footer />
    </ShadcnSidebar>
  );
}
