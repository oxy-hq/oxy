import { ChevronsLeft, DiamondPlus, Home } from "lucide-react";
import { Link, useLocation } from "react-router-dom";
import { Button } from "@/components/ui/shadcn/button";
import { SidebarMenu, SidebarMenuButton, SidebarMenuItem } from "@/components/ui/shadcn/sidebar";
import useSidebar from "@/components/ui/shadcn/sidebar-context";
import ROUTES from "@/libs/utils/routes";
import useCurrentWorkspace from "@/stores/useCurrentWorkspace";
import ContextGraph from "./ContextGraph";
import Ide from "./Ide";
import { WorkspaceSwitcher } from "./WorkspaceSwitcher";

export function Header() {
  const location = useLocation();
  const { toggleSidebar, open } = useSidebar();
  const { workspace } = useCurrentWorkspace();
  const workspaceId = workspace?.id ?? "";
  const homeUri = ROUTES.WORKSPACE(workspaceId).HOME;
  const isHome = location.pathname === homeUri || location.pathname === "/";

  return (
    <div className='flex flex-col'>
      {/* Brand bar: logo + project switcher + collapse */}
      <div className='flex h-[52px] shrink-0 items-center gap-0 border-sidebar-border/50 border-b px-3'>
        <Link to={homeUri} className='flex shrink-0 items-center pr-2'>
          <img src='/oxy-light.svg' alt='Oxy' className='dark:hidden' />
          <img src='/oxy-dark.svg' alt='Oxy' className='hidden dark:block' />
        </Link>

        <div className='min-w-0 flex-1'>
          <WorkspaceSwitcher />
        </div>

        {open && (
          <Button
            onClick={toggleSidebar}
            variant='ghost'
            size='icon'
            className='ml-1 h-7 w-7 shrink-0 p-0 text-sidebar-foreground/25 hover:bg-sidebar-accent hover:text-sidebar-foreground/60'
          >
            <ChevronsLeft className='h-4 w-4' />
          </Button>
        )}
      </div>

      {/* Primary navigation */}
      <div className='px-2 py-2'>
        <SidebarMenu>
          <SidebarMenuItem>
            <SidebarMenuButton
              asChild
              className='h-8 gap-2.5 rounded-md px-2.5 font-medium text-[13px]'
            >
              <Link to={homeUri}>
                <DiamondPlus className='h-[15px] w-[15px] shrink-0' />
                <span data-testid='start-new-thread'>Start new thread</span>
              </Link>
            </SidebarMenuButton>
          </SidebarMenuItem>
          <SidebarMenuItem>
            <SidebarMenuButton
              asChild
              isActive={isHome}
              className='h-8 gap-2.5 rounded-md px-2.5 font-medium text-[13px]'
            >
              <Link to={homeUri}>
                <Home className='h-[15px] w-[15px] shrink-0' />
                <span>Home</span>
              </Link>
            </SidebarMenuButton>
          </SidebarMenuItem>
          <Ide />
          <ContextGraph />
        </SidebarMenu>
      </div>
    </div>
  );
}
