import { AppWindow, Building2, ScrollText, UserCog } from "lucide-react";
import type { ComponentType } from "react";
import { Link, useLocation } from "react-router-dom";
import OxyLogo from "@/components/OxyLogo";
import {
  Sidebar as ShadcnSidebar,
  SidebarGroup,
  SidebarGroupLabel,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem
} from "@/components/ui/shadcn/sidebar";
import ROUTES from "@/libs/utils/routes";
import { usePartnerConsole } from "../../context";

type NavItem = {
  to: string;
  label: string;
  icon: ComponentType<{ className?: string }>;
  /** Rendered only when this person can actually use it. */
  visible: (v: ReturnType<typeof usePartnerConsole>["active"]) => boolean;
};

/**
 * Same shape as the admin sidebar, because it is the same *kind* of surface: an
 * operations console for someone administering other people's organizations. The
 * only difference is reach.
 *
 * One flat, four-item list — Clients and Custom apps are the two nouns a partner
 * works in; Team and Activity are the supporting surfaces. Workspace health used
 * to be its own item, but it's inherently per-client, so it now lives inside each
 * client's detail. Items are capability-driven — an item nobody can use is not
 * rendered rather than rendered and then 403'd. (The server re-checks regardless.)
 */
const NAV: NavItem[] = [
  {
    to: ROUTES.PARTNERS.ROOT,
    label: "Clients",
    icon: Building2,
    visible: () => true
  },
  {
    to: ROUTES.PARTNERS.APPS,
    label: "Custom apps",
    icon: AppWindow,
    visible: (p) => p.capabilities.manage_apps
  },
  {
    to: ROUTES.PARTNERS.TEAM,
    label: "Team",
    icon: UserCog,
    // Visible to every operator. Granting access is the org owner/admin's job — the
    // server 403s a plain member's toggle — but the roster itself is fine to show.
    visible: () => true
  },
  {
    to: ROUTES.PARTNERS.ACTIVITY,
    label: "Activity",
    icon: ScrollText,
    visible: (p) => p.capabilities.view_audit
  }
];

export function PartnerSidebar() {
  const location = useLocation();
  const { active } = usePartnerConsole();
  const items = NAV.filter((i) => i.visible(active));

  return (
    <ShadcnSidebar className='border-sidebar-border border-r bg-sidebar-background'>
      <div className='flex h-[52px] shrink-0 items-center gap-2 border-sidebar-border/50 border-b px-3'>
        <Link to={ROUTES.PARTNERS.ROOT} className='flex shrink-0 items-center'>
          <OxyLogo />
        </Link>
        <span className='rounded-sm border border-sidebar-border px-1.5 py-0.5 font-mono text-[10px] text-muted-foreground uppercase tracking-[0.2em]'>
          Partner
        </span>
      </div>

      <div className='min-h-0 flex-1 overflow-auto'>
        <SidebarGroup className='px-2 pt-2'>
          <SidebarGroupLabel>{active.name}</SidebarGroupLabel>
          <SidebarMenu>
            {items.map(({ to, label, icon: Icon }) => (
              <SidebarMenuItem key={to}>
                <SidebarMenuButton
                  asChild
                  // ROOT is a prefix of every other partner route, so it only
                  // counts as active on an exact match.
                  isActive={
                    to === ROUTES.PARTNERS.ROOT
                      ? location.pathname === to
                      : location.pathname.startsWith(to)
                  }
                  className='gap-2.5 text-[13px] data-[active=true]:font-medium [&>svg]:size-3.5'
                >
                  <Link to={to}>
                    <Icon />
                    <span className='tracking-tight'>{label}</span>
                  </Link>
                </SidebarMenuButton>
              </SidebarMenuItem>
            ))}
          </SidebarMenu>
        </SidebarGroup>
      </div>
    </ShadcnSidebar>
  );
}
