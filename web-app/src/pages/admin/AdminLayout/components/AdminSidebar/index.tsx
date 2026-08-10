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

export type AdminNavItem = {
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

/**
 * Every admin route and the standing it needs. **The one map** — `AdminLayout`'s
 * route guard reads it too, via [`canReachAdminRoute`].
 *
 * It was not the one map: the layout carried its own hardcoded list of path prefixes a
 * non-owner could reach, written before capabilities existed. Adding a capability to a
 * nav item made it appear and then bounce, because the two lists disagreed — which is
 * how `Staff access` shipped visible and unreachable for every Global Admin.
 */
export const ADMIN_NAV: AdminNavItem[] = [
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
    // Matches the page's own <h1>. "Global admins" predates App Operator and named
    // one of the two roles the page administers, so the nav read as a different
    // surface from the one it opened.
    label: "Staff access",
    icon: ShieldCheck,
    capability: "manage_platform_grants",
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

/** The standing a nav rule is evaluated against. */
type Standing = { isOwner: boolean; capabilities: PlatformCapability[] };

/** The rule for ONE entry. Owner-only rooms are a boolean the capability model
 * deliberately cannot reach; everything else names a capability; an entry with neither is
 * open to any staff member who got through the console door. */
function itemReachable(item: AdminNavItem, { isOwner, capabilities }: Standing): boolean {
  if (item.ownerOnly) return isOwner;
  // Root satisfies every capability, the same short-circuit `may_delegate` and
  // `platform_grants` apply server-side. Reading only `capabilities` happened to work
  // because `/user` sends the owner `Cap::ALL` — a server implementation detail this
  // file should not be leaning on, and one that would blank the owner's own console the
  // day that read fails and returns an empty list.
  if (item.capability) return isOwner || capabilities.includes(item.capability);
  return true;
}

/** An entry's path, without the query it carries for the directory's `?type=` tabs. */
const navPath = (to: string) => to.split("?")[0];

/**
 * May this principal reach `pathname`? The route-guard half of the same map the sidebar
 * filters on, so a visible item is always a reachable one.
 *
 * **Not the per-item rule.** Three entries — Organizations, Partners, Users — are all
 * `/admin/tenants` with three *different* capabilities, so "longest match wins, then
 * apply its rule" would bounce someone holding `manage_members` but not
 * `manage_org_settings` off a page they can plainly use. The route is reachable if **any**
 * entry pointing at it is. The sidebar still decides per item, which is why the two
 * cannot share one rule verbatim: one asks "may I see this link", the other "may I be on
 * this page".
 *
 * The query string has to come off before matching. With it left on, `i.to` was
 * `/admin/tenants?type=orgs` and `location.pathname` is `/admin/tenants`, so no tenant
 * entry could ever match, `match` was undefined, and the guard returned `true`
 * unconditionally for the largest group in the map — a rule stated in a comment that the
 * code did not apply, which is the defect this function was written to end.
 *
 * Unknown paths return `true`: this is a redirect for a stale bookmark, not an
 * authorization control — the server decides, and guessing "deny" would bounce a route
 * that simply is not in the nav (a detail page, say).
 */
export function canReachAdminRoute(pathname: string, standing: Standing): boolean {
  const candidates = ADMIN_NAV.filter((i) => {
    const p = navPath(i.to);
    // Segment boundary, so `/admin/apps/<id>` inherits `/admin/apps` but a future
    // `/admin/apps-registry` does not.
    return pathname === p || pathname.startsWith(`${p}/`);
  });
  if (candidates.length === 0) return true;

  // Most specific path wins; every entry AT that path gets a vote.
  const longest = Math.max(...candidates.map((i) => navPath(i.to).length));
  return candidates
    .filter((i) => navPath(i.to).length === longest)
    .some((i) => itemReachable(i, standing));
}

/**
 * The first admin route this principal can actually use — where to send someone who
 * landed somewhere they cannot be.
 *
 * `AdminLayout` bounced to Custom apps, which is itself gated on `manage_apps`. Every
 * role shipping today holds it (Global Admin via `Cap::ALL - ManageBilling`, App Operator
 * by definition), so the bounce lands. But the point of this branch is that a narrower
 * preset is now cheap to add, and the first one that omits `manage_apps` — an audit-only
 * or grants-only role — would `Navigate` to a page the guard immediately bounces it off
 * again. A redirect cycle, not a bounce.
 *
 * The sidebar already needed this for its logo link, "so each role lands somewhere it can
 * actually use". Same map, same rule, one definition.
 */
export function firstReachableAdminRoute(standing: Standing): string {
  // `/` rather than a hardcoded admin route: a principal with nothing visible has no
  // admin landing place, and the layout guard above sends them home anyway.
  return ADMIN_NAV.find((i) => itemReachable(i, standing))?.to ?? "/";
}

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
  // Per item, deliberately — `canReachAdminRoute` answers a different question (see its
  // doc): on `/admin/tenants` any one of three capabilities admits you to the page, but
  // each rail link still shows only to whoever holds its own.
  const visibleItems = ADMIN_NAV.filter((item) => itemReachable(item, { isOwner, capabilities }));

  const logoTarget = firstReachableAdminRoute({ isOwner, capabilities });

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
