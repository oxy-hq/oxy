import { ChevronsLeft, ChevronsRight, FolderKanban, Settings, Users } from "lucide-react";
import { Link, useLocation, useParams } from "react-router-dom";
import { Footer } from "@/components/AppSidebar/Footer";
import { Button } from "@/components/ui/shadcn/button";
import {
  Sidebar as ShadcnSidebar,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem
} from "@/components/ui/shadcn/sidebar";
import useSidebar from "@/components/ui/shadcn/sidebar-context";
import ROUTES from "@/libs/utils/routes";
import useCurrentOrg from "@/stores/useCurrentOrg";

export default function OrgSidebar() {
  const { orgSlug } = useParams<{ orgSlug: string }>();
  const { pathname } = useLocation();
  const { toggleSidebar, open } = useSidebar();
  const orgName = useCurrentOrg((s) => s.org?.name) ?? "";

  if (!orgSlug) return null;

  const routes = ROUTES.ORG(orgSlug);

  const items = [
    { label: "Workspaces", icon: FolderKanban, to: routes.WORKSPACES },
    { label: "Team", icon: Users, to: routes.MEMBERS },
    { label: "Settings", icon: Settings, to: routes.SETTINGS }
  ];

  return (
    <ShadcnSidebar
      collapsible='icon'
      className='border-sidebar-border border-r bg-sidebar-background'
    >
      <div className='flex flex-col'>
        {/* Brand bar: logo + org name + collapse (all hidden in icon mode) */}
        <div className='flex h-[52px] shrink-0 items-center gap-0 border-sidebar-border/50 border-b px-3 group-data-[collapsible=icon]:justify-center group-data-[collapsible=icon]:px-0'>
          <Link
            to={ROUTES.ROOT}
            className='flex shrink-0 items-center pr-2 group-data-[collapsible=icon]:pr-0'
          >
            <img src='/oxy-light.svg' alt='Oxy' className='dark:hidden' />
            <img src='/oxy-dark.svg' alt='Oxy' className='hidden dark:block' />
          </Link>

          <div className='flex min-w-0 flex-1 items-center px-2 py-1.5 group-data-[collapsible=icon]:hidden'>
            <span className='flex-1 truncate text-left font-semibold text-[13px] text-sidebar-foreground'>
              {orgName}
            </span>
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
            {items.map((item) => (
              <SidebarMenuItem key={item.label}>
                <SidebarMenuButton
                  asChild
                  isActive={pathname.startsWith(item.to)}
                  tooltip={item.label}
                  className='h-8 gap-2.5 rounded-md px-2.5 font-medium text-[13px]'
                >
                  <Link to={item.to}>
                    <item.icon className='h-[15px] w-[15px] shrink-0' />
                    <span>{item.label}</span>
                  </Link>
                </SidebarMenuButton>
              </SidebarMenuItem>
            ))}
          </SidebarMenu>
        </div>
      </div>

      <div className='min-h-0 flex-1' />

      {/* Bottom expand toggle: only visible when collapsed */}
      {!open && (
        <div className='p-2'>
          <Button
            onClick={toggleSidebar}
            variant='ghost'
            size='icon'
            className='h-8 w-full text-sidebar-foreground/60 hover:bg-sidebar-accent hover:text-sidebar-foreground'
          >
            <ChevronsRight className='h-4 w-4' />
          </Button>
        </div>
      )}

      <Footer />
    </ShadcnSidebar>
  );
}
