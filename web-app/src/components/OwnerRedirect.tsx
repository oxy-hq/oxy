import { Navigate, Outlet } from "react-router-dom";
import { Spinner } from "@/components/ui/shadcn/spinner";
import useCurrentUser from "@/hooks/api/users/useCurrentUser";
import ROUTES from "@/libs/utils/routes";

/**
 * Wraps non-admin auth-gated routes and bounces OXY_OWNER users back to the
 * admin shell. The login callbacks already pick the admin queue as the
 * destination via `handlePostLoginOrgs`, so this only catches manual
 * navigation (typing a URL, browser back, or stale links). The server-side
 * `oxy_owner_guard` middleware remains the authoritative gate for admin
 * endpoints — this guard is UX-only.
 */
export default function OwnerRedirect() {
  const { data: user, isPending } = useCurrentUser();

  if (isPending) {
    return (
      <div className='flex h-full w-full items-center justify-center'>
        <Spinner className='size-6' />
      </div>
    );
  }

  if (user?.is_owner) {
    return <Navigate to={ROUTES.ADMIN.BILLING_QUEUE} replace />;
  }

  return <Outlet />;
}
