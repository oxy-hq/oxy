import { Navigate, Outlet, useLocation } from "react-router-dom";
import { SidebarInset, SidebarProvider } from "@/components/ui/shadcn/sidebar";
import { Spinner } from "@/components/ui/shadcn/spinner";
import useCurrentUser from "@/hooks/api/users/useCurrentUser";
import ROUTES from "@/libs/utils/routes";
import { AdminSidebar } from "./components/AdminSidebar";
import { AdminTopbar } from "./components/AdminTopbar";

const PAGE_TITLES: Record<string, string> = {
  "/admin/billing/queue": "Billing queue",
  "/admin/feature-flags": "Feature flags",
  "/admin/app-admins": "App admins",
  "/admin/apps": "Customer apps"
};

// Per-route access rules. App admins (OXY_APP_ADMINS, not in OXY_OWNER) get
// access to `/admin/apps` and its sub-routes (the master-detail view uses
// `/admin/apps/:org/:slug`) but not the owner-only operational pages —
// without this gate they'd land on a 403'd Billing queue and assume the
// whole admin surface is broken.
const APP_ADMIN_ROUTE_PREFIXES = ["/admin/apps"];

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
