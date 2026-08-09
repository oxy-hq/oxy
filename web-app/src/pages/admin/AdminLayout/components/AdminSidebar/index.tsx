import {
  Activity,
  AppWindow,
  Building2,
  FileCheck,
  Flag,
  Handshake,
  HeartPulse,
  Inbox,
  ScrollText,
  ShieldCheck,
  Telescope,
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
import { useWorkspaceHealth } from "@/hooks/api/workspaceHealth/useWorkspaceHealth";
import { cn } from "@/libs/shadcn/utils";
import ROUTES from "@/libs/utils/routes";
import type { PlatformCapability } from "@/types/auth";
import { Footer } from "./components/Footer";

type AdminNavItem = {
  to: string;
  label: string;
  icon: ComponentType<{ className?: string }>;
  /** When true, render only for Global Owners (the OXY_OWNER env-var allow-list). */
  ownerOnly?: boolean;
  /**
   * The platform capability this page needs — the same one its router gate names in
   * `crates/app/src/server/api/admin/mod.rs`. Keeping the two in step is what stops the
   * nav from offering a room the server will 403; when they drift, the server wins and
   * the user gets a dead link.
   */
  capability?: PlatformCapability;
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
    capability: "operate_platform",
    group: "operations"
  },
  {
    to: ROUTES.ADMIN.INTERNAL_JOBS,
    label: "Internal jobs",
    icon: Activity,
    capability: "operate_platform",
    group: "operations"
  },
  {
    to: ROUTES.ADMIN.COMPILES,
    label: "Compile revisions",
    icon: FileCheck,
    capability: "operate_platform",
    group: "operations"
  },
  {
    to: ROUTES.ADMIN.EXPLORER,
    label: "Explorer",
    icon: Telescope,
    capability: "view_tenants",
    group: "operations"
  },
  {
    to: ROUTES.ADMIN.AUDIT,
    label: "Audit log",
    icon: ScrollText,
    capability: "view_audit",
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
    capability: "manage_apps",
    group: "operations"
  },
  // Publish tokens now lives as a tab inside Custom apps (/admin/apps?view=tokens),
  // not its own nav item — it's part of shipping apps, not a separate surface.
  {
    to: ROUTES.ADMIN.WORKSPACE_HEALTH,
    label: "Workspace health",
    icon: HeartPulse,
    capability: "operate_platform",
    group: "operations"
  },
  // Tenant management: the unified, relationship-first directory of orgs /
  // partners / users (workspaces live one level down, inside their org). Each
  // entry is a shortcut to one entity type of the SAME surface — clicking
  // "Partners" here is identical to picking Partners in the directory's own
  // header switcher (both just drive `?type=`). Open to owner OR app admin.
  {
    to: `${ROUTES.ADMIN.TENANTS}?type=orgs`,
    label: "Organizations",
    icon: Building2,
    capability: "manage_org_settings",
    group: "tenants"
  },
  {
    to: `${ROUTES.ADMIN.TENANTS}?type=partners`,
    label: "Partners",
    icon: Handshake,
    capability: "manage_partners",
    group: "tenants"
  },
  {
    to: `${ROUTES.ADMIN.TENANTS}?type=users`,
    label: "Users",
    icon: Users,
    capability: "manage_members",
    group: "tenants"
  }
];

export function AdminSidebar() {
  const location = useLocation();
  // The directory's active entity type, so the tenant nav shortcuts light up in
  // lockstep with the directory's own header switcher (both read `?type=`).
  const currentTenantType = new URLSearchParams(location.search).get("type") ?? "orgs";
  const { data: user } = useCurrentUser();
  const isOwner = user?.is_owner ?? false;
  const capabilities = user?.platform_capabilities ?? [];

  // Surface a count of workspaces needing attention right on the nav item,
  // so operators see trouble without opening the Workspace health page.
  // Same 30s-stale rollup the health page reads — worst-first, cross-tenant.
  const { data: health } = useWorkspaceHealth();
  const attentionCount = health?.workspaces.filter((ws) => ws.status !== "healthy").length ?? 0;
  const hasUnhealthy = health?.workspaces.some((ws) => ws.status === "unhealthy") ?? false;

  // One rule per item, in the same order the server applies them: owner-only rooms are
  // a boolean the capability model deliberately cannot reach (the Billing queue and the
  // grant table itself); everything else asks for a capability. An item with neither is
  // open to any staff member who got through the console door.
  const visibleItems = ADMIN_NAV.filter((item) => {
    if (item.ownerOnly) return isOwner;
    if (item.capability) return capabilities.includes(item.capability);
    return true;
  });

  // Logo link goes to the first visible admin route, so each role lands somewhere it
  // can actually use: an App Operator on Custom apps, an owner on the billing queue.
  // Falls back to `/` only when the user has no admin access at all — the layout-level
  // guard should have already redirected them in that case.
  const logoTarget = visibleItems[0]?.to ?? "/";

  return (
    <ShadcnSidebar className='border-sidebar-border border-r bg-sidebar-background'>
      <div className='flex h-[52px] shrink-0 items-center gap-2 border-sidebar-border/50 border-b px-3'>
        <Link to={logoTarget} className='flex shrink-0 items-center'>
          <OxyLogo />
        </Link>
        <span className='rounded-sm border border-sidebar-border px-1.5 py-0.5 font-mono text-[10px] text-muted-foreground uppercase tracking-[0.2em]'>
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
                  // Tenant items carry a `?type=` query and share one pathname,
                  // so match on the active type; everything else matches by path.
                  const [itemPath, itemQuery] = to.split("?");
                  const itemType = itemQuery ? new URLSearchParams(itemQuery).get("type") : null;
                  const isActive = itemType
                    ? location.pathname === ROUTES.ADMIN.TENANTS && currentTenantType === itemType
                    : location.pathname.startsWith(itemPath);
                  const showHealthBadge =
                    to === ROUTES.ADMIN.WORKSPACE_HEALTH && attentionCount > 0;
                  return (
                    <SidebarMenuItem key={to}>
                      <SidebarMenuButton
                        asChild
                        isActive={isActive}
                        className='gap-2.5 text-[13px] data-[active=true]:font-medium [&>svg]:size-3.5'
                      >
                        <Link to={to}>
                          <Icon />
                          <span className='tracking-tight'>{label}</span>
                          {showHealthBadge && (
                            <span
                              data-testid='workspace-health-nav-badge'
                              title={`${attentionCount} workspace${attentionCount === 1 ? "" : "s"} need attention`}
                              className={cn(
                                "ml-auto inline-flex h-4 min-w-4 items-center justify-center rounded-full px-1 font-medium text-[10px] tabular-nums ring-1 ring-inset",
                                hasUnhealthy
                                  ? "bg-destructive/10 text-destructive ring-destructive/20"
                                  : "bg-amber-500/10 text-amber-700 ring-amber-500/20 dark:text-amber-400"
                              )}
                            >
                              {attentionCount}
                            </span>
                          )}
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
