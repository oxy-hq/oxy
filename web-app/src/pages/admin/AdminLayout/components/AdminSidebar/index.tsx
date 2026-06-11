import {
  Activity,
  AppWindow,
  Building2,
  FileCheck,
  Flag,
  FolderOpen,
  Inbox,
  LayoutDashboard,
  ShieldCheck,
  Users
} from "lucide-react";
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
  /** When true, render only for Global Owners (the OXY_OWNER env-var allow-list). */
  ownerOnly?: boolean;
  /** When true, render for either Global Owners or Global Admins (`app_admins` table). */
  adminOrAppAdmin?: boolean;
  /** Logical grouping label shown above the items. */
  group: "operations" | "tenants";
};

const ADMIN_NAV: AdminNavItem[] = [
  // Billing queue is strict Global Owner — "billing adjustment" per the
  // server-side route_layer in admin/mod.rs (OXY_OWNER env-var allow-list).
  {
    to: ROUTES.ADMIN.BILLING_QUEUE,
    label: "Billing queue",
    icon: Inbox,
    ownerOnly: true,
    group: "operations"
  },
  {
    to: ROUTES.ADMIN.FEATURE_FLAGS,
    label: "Feature flags",
    icon: Flag,
    adminOrAppAdmin: true,
    group: "operations"
  },
  {
    to: ROUTES.ADMIN.INTERNAL_JOBS,
    label: "Internal jobs",
    icon: Activity,
    adminOrAppAdmin: true,
    group: "operations"
  },
  {
    to: ROUTES.ADMIN.COMPILES,
    label: "Compile revisions",
    icon: FileCheck,
    adminOrAppAdmin: true,
    group: "operations"
  },
  // "Global admins" manages the `app_admins` table itself — "promotion /
  // demotion of admin", strict Global Owner only.
  {
    to: ROUTES.ADMIN.APP_ADMINS,
    label: "Global admins",
    icon: ShieldCheck,
    ownerOnly: true,
    group: "operations"
  },
  {
    to: ROUTES.ADMIN.CUSTOMER_APPS,
    label: "Custom apps",
    icon: AppWindow,
    adminOrAppAdmin: true,
    group: "operations"
  },
  // Tenant management: cross-cutting directory of orgs / users / workspaces.
  // Open to owner OR app admin — both flavors of operator triage tenants.
  // Overview hub at the top of the group is the natural landing surface
  // for tenant ops; the focused list pages remain the place to act.
  {
    to: ROUTES.ADMIN.TENANTS,
    label: "Overview",
    icon: LayoutDashboard,
    adminOrAppAdmin: true,
    group: "tenants"
  },
  {
    to: ROUTES.ADMIN.ORGS,
    label: "Organizations",
    icon: Building2,
    adminOrAppAdmin: true,
    group: "tenants"
  },
  {
    to: ROUTES.ADMIN.USERS,
    label: "Users",
    icon: Users,
    adminOrAppAdmin: true,
    group: "tenants"
  },
  {
    to: ROUTES.ADMIN.WORKSPACES,
    label: "Workspaces",
    icon: FolderOpen,
    adminOrAppAdmin: true,
    group: "tenants"
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
        {(["operations", "tenants"] as const).map((group) => {
          const items = visibleItems.filter((i) => i.group === group);
          if (items.length === 0) return null;
          const groupLabel = group === "operations" ? "Operations" : "Tenants";
          return (
            <SidebarGroup key={group} className='px-2 pt-2'>
              <SidebarGroupLabel>{groupLabel}</SidebarGroupLabel>
              <SidebarMenu>
                {items.map(({ to, label, icon: Icon }) => {
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
          );
        })}
      </div>

      <Footer />
    </ShadcnSidebar>
  );
}
