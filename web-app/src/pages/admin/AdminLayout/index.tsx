import { Navigate, Outlet, useLocation } from "react-router-dom";
import { SidebarInset, SidebarProvider } from "@/components/ui/shadcn/sidebar";
import { Spinner } from "@/components/ui/shadcn/spinner";
import { useActingSession } from "@/hooks/api/adminAssume/useActingSession";
import useCurrentUser from "@/hooks/api/users/useCurrentUser";
import ROUTES from "@/libs/utils/routes";
import { AdminSidebar } from "./components/AdminSidebar";
import { AdminTopbar } from "./components/AdminTopbar";

const PAGE_TITLES: Record<string, string> = {
  [ROUTES.ADMIN.BILLING_QUEUE]: "Billing queue",
  [ROUTES.ADMIN.FEATURE_FLAGS]: "Feature flags",
  [ROUTES.ADMIN.INTERNAL_JOBS]: "Internal jobs",
  [ROUTES.ADMIN.COMPILES]: "Compile revisions",
  [ROUTES.ADMIN.EXPLORER]: "Explorer",
  [ROUTES.ADMIN.AUDIT]: "Audit log",
  [ROUTES.ADMIN.APP_ADMINS]: "Global admins",
  [ROUTES.ADMIN.PUBLISH_TOKENS]: "Publish tokens",
  [ROUTES.ADMIN.CUSTOMER_APPS]: "Custom apps",
  [ROUTES.ADMIN.WORKSPACE_HEALTH]: "Workspace health",
  [ROUTES.ADMIN.TENANTS]: "Tenants overview",
  [ROUTES.ADMIN.ORGS]: "Organizations",
  [ROUTES.ADMIN.USERS]: "Users",
  [ROUTES.ADMIN.WORKSPACES]: "Workspaces"
};

// Routes that Global Admins (members of the `app_admins` table) are
// allowed to reach. Keep in sync with the per-route guards in
// `crates/app/src/server/api/admin/mod.rs` — anything NOT in this list is
// reserved for Global Owner (today: Billing queue + Global admins).
// Global admins who land on an owner-only route get bounced to Customer
// apps so they don't hit a 403 page and assume the whole admin surface is
// broken.
// Backend identifiers (OXY_OWNER env var, `app_admins` table, the
// `is_owner` / `is_app_admin` API fields) keep their on-the-wire names.
const APP_ADMIN_ROUTE_PREFIXES = [
  ROUTES.ADMIN.CUSTOMER_APPS,
  ROUTES.ADMIN.PUBLISH_TOKENS,
  ROUTES.ADMIN.INTERNAL_JOBS,
  ROUTES.ADMIN.COMPILES,
  ROUTES.ADMIN.EXPLORER,
  ROUTES.ADMIN.AUDIT,
  ROUTES.ADMIN.FEATURE_FLAGS,
  ROUTES.ADMIN.WORKSPACE_HEALTH,
  ROUTES.ADMIN.TENANTS,
  ROUTES.ADMIN.ORGS,
  ROUTES.ADMIN.USERS,
  ROUTES.ADMIN.WORKSPACES
];

function isAppAdminRoute(pathname: string): boolean {
  return APP_ADMIN_ROUTE_PREFIXES.some(
    (prefix) => pathname === prefix || pathname.startsWith(`${prefix}/`)
  );
}

export default function AdminLayout() {
  const location = useLocation();
  const { data: user, isPending } = useCurrentUser();
  // Exact-match first, then prefix-match so `/admin/apps/:org/:slug` still
  // shows "Customer apps" in the topbar instead of falling back to "Admin".
  const title =
    PAGE_TITLES[location.pathname] ??
    Object.entries(PAGE_TITLES).find(([prefix]) =>
      location.pathname.startsWith(`${prefix}/`)
    )?.[1] ??
    "Admin";

  const acting = useActingSession();

  if (isPending) {
    return (
      <div className='flex h-full w-full items-center justify-center'>
        <Spinner className='size-6' />
      </div>
    );
  }

  const isOwner = user?.is_owner ?? false;
  const isAppAdmin = user?.is_app_admin ?? false;

  // No admin role at all — go home. Avoids a "Failed to load" stuck state
  // for users who land on `/admin/apps` via a stale bookmark.
  if (!isOwner && !isAppAdmin) {
    return <Navigate to='/' replace />;
  }

  // App-admin-only user trying to reach an owner-only page — redirect them
  // to the customer-apps page they actually have access to.
  if (!isOwner && !isAppAdminRoute(location.pathname)) {
    return <Navigate to={ROUTES.ADMIN.CUSTOMER_APPS} replace />;
  }

  // You cannot be in here while acting as a tenant. The server already refuses
  // the whole staff surface (assume::block_admin_while_acting), so rendering the
  // admin shell would just paint a page of 403s — but the deeper reason is that
  // "acting as" only means something if it changes where you are and what you can
  // do. Sitting in the admin panel with a banner on top changed neither.
  //
  // The banner's Stop button ends the session and brings you straight back.
  if (acting.isActing && acting.landing) {
    return <Navigate to={acting.landing} replace />;
  }

  return (
    <SidebarProvider>
      <AdminSidebar />
      <SidebarInset>
        <AdminTopbar title={title} />
        <div className='min-h-0 flex-1 overflow-auto'>
          <Outlet />
        </div>
      </SidebarInset>
    </SidebarProvider>
  );
}
