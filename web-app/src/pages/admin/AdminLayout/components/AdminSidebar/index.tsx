import { AppWindow, Flag, Inbox, ShieldCheck } from "lucide-react";
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
import useCurrentUser from "@/hooks/api/users/useCurrentUser";
import ROUTES from "@/libs/utils/routes";
import { Footer } from "./components/Footer";

type AdminNavItem = {
  to: string;
  label: string;
  icon: ComponentType<{ className?: string }>;
  /** When true, render only for Oxy owners (OXY_OWNER). */
  ownerOnly?: boolean;
  /** When true, render for either OXY_OWNER or members of the app_admins table. */
  adminOrAppAdmin?: boolean;
};

const ADMIN_NAV: AdminNavItem[] = [
  { to: ROUTES.ADMIN.BILLING_QUEUE, label: "Billing queue", icon: Inbox, ownerOnly: true },
  { to: ROUTES.ADMIN.FEATURE_FLAGS, label: "Feature flags", icon: Flag, ownerOnly: true },
  { to: ROUTES.ADMIN.APP_ADMINS, label: "App admins", icon: ShieldCheck, ownerOnly: true },
  {
    to: ROUTES.ADMIN.CUSTOMER_APPS,
    label: "Customer apps",
    icon: AppWindow,
    adminOrAppAdmin: true
  }
];

export function AdminSidebar() {
  const location = useLocation();
  const { data: user } = useCurrentUser();
  const isOwner = user?.is_owner ?? false;
  const isAppAdmin = user?.is_app_admin ?? false;

  const visibleItems = ADMIN_NAV.filter((item) => {
    if (item.ownerOnly) return isOwner;
    if (item.adminOrAppAdmin) return isOwner || isAppAdmin;
    return true;
  });

  // Logo link goes to the first visible admin route. App-admin-only users
  // land on Customer apps (their only admin surface); owners land on the
  // billing queue (their operational home). Falls back to `/` only when the
  // user has no admin access at all — the layout-level guard should have
  // already redirected them in that case.
  const logoTarget = visibleItems[0]?.to ?? "/";

  return (
    <ShadcnSidebar className='border-sidebar-border border-r bg-sidebar-background'>
      <div className='flex h-[52px] shrink-0 items-center gap-2 border-sidebar-border/50 border-b px-3'>
        <Link to={logoTarget} className='flex shrink-0 items-center'>
          <OxyLogo />
        </Link>
        <span className='rounded bg-muted px-1.5 py-0.5 font-medium text-[10px] text-muted-foreground uppercase tracking-wider'>
          Admin
        </span>
      </div>

      <div className='min-h-0 flex-1 overflow-auto'>
        <SidebarGroup className='px-2 pt-2'>
          <SidebarGroupLabel>Operations</SidebarGroupLabel>
          <SidebarMenu>
            {visibleItems.map(({ to, label, icon: Icon }) => {
              const isActive = location.pathname.startsWith(to);
              return (
                <SidebarMenuItem key={to}>
                  <SidebarMenuButton asChild isActive={isActive}>
                    <Link to={to}>
                      <Icon />
                      <span>{label}</span>
                    </Link>
                  </SidebarMenuButton>
                </SidebarMenuItem>
              );
            })}
          </SidebarMenu>
        </SidebarGroup>
      </div>

      <Footer />
    </ShadcnSidebar>
  );
}
