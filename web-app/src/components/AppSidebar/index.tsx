import {
  Sidebar as ShadcnSidebar,
  SidebarGroup,
  SidebarGroupLabel,
  SidebarMenu
} from "@/components/ui/shadcn/sidebar";
import { Apps } from "./Apps";
import { Footer } from "./Footer";
import { Header } from "./Header";
import Threads from "./Threads";
import { Workflows } from "./Workflows";

export function AppSidebar() {
  return (
    <ShadcnSidebar className='bg-sidebar-background'>
      <Header />

      <div className='customScrollbar scrollbar-gutter-auto flex min-h-0 flex-1 flex-col overflow-auto'>
        <SidebarGroup className='px-2 pt-0'>
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
