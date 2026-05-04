import { useEffect } from "react";
import { Navigate, Outlet, useLocation, useParams } from "react-router-dom";
import { BillingBanner } from "@/components/billing/BillingBanner";
import { PaywallScreen } from "@/components/billing/PaywallScreen";
import { Spinner } from "@/components/ui/shadcn/spinner";
import { useOrgBillingStatus } from "@/hooks/api/billing";
import { useOrgs } from "@/hooks/api/organizations";
import { setLastOrgSlug } from "@/libs/utils/lastWorkspace";
import ROUTES from "@/libs/utils/routes";
import useCurrentOrg from "@/stores/useCurrentOrg";
import { usePaywallStore } from "@/stores/usePaywallStore";

/**
 * Route guard for /:orgSlug/* routes.
 *
 * - Resolves org from slug, verifies user is a member, sets Zustand store.
 * - Mounts `PaywallScreen` instead of the app shell when the org's billing
 *   status doesn't grant access (`incomplete`, `unpaid`, `canceled`, or
 *   `past_due` past grace as observed via the latest 402 response).
 * - The `/billing/*` sub-routes always pass through so admins can still pay.
 */
export default function OrgGuard() {
  const { orgSlug } = useParams<{ orgSlug: string }>();
  const location = useLocation();
  const { data: orgs, isPending } = useOrgs();
  const { org: currentOrg, role, setOrg, clearOrg } = useCurrentOrg();

  const matchedOrg = orgs?.find((o) => o.slug === orgSlug);
  const hasWorkspaces = (matchedOrg?.workspace_count ?? 0) > 0;

  const { data: billing, isPending: billingPending } = useOrgBillingStatus(
    matchedOrg?.id ?? "",
    Boolean(matchedOrg),
    // While the org is in the past_due grace window we render BillingBanner,
    // not the paywall. The 402 axios interceptor still flips the paywall
    // store on the first blocked request, but a refetch here closes the
    // race between grace expiring on the server and the cached "past_due"
    // status the client is rendering.
    {
      refetchInterval: (query) => (query.state.data?.status === "past_due" ? 60_000 : false)
    }
  );
  const paywallOpen = usePaywallStore((s) => s.open);
  const closePaywall = usePaywallStore((s) => s.close);

  const onBillingPath = location.pathname.includes("/billing/");
  const blocked =
    billing &&
    !onBillingPath &&
    (billing.status === "incomplete" ||
      billing.status === "unpaid" ||
      billing.status === "canceled" ||
      paywallOpen);

  useEffect(() => {
    if (matchedOrg) {
      if (matchedOrg.id !== currentOrg?.id) {
        setOrg(matchedOrg);
      }
      if (hasWorkspaces) {
        setLastOrgSlug(matchedOrg.slug);
      }
    }
    if (!isPending && !matchedOrg && currentOrg?.slug === orgSlug) {
      clearOrg();
    }
  }, [
    matchedOrg,
    currentOrg?.id,
    currentOrg?.slug,
    orgSlug,
    isPending,
    hasWorkspaces,
    setOrg,
    clearOrg
  ]);

  // If the user navigates to /billing/* themselves, retire any stale paywall
  // store flag so the page renders normally.
  useEffect(() => {
    if (onBillingPath && paywallOpen) {
      closePaywall();
    }
  }, [onBillingPath, paywallOpen, closePaywall]);

  if (isPending) {
    return (
      <div className='flex h-full w-full items-center justify-center'>
        <Spinner className='size-6' />
      </div>
    );
  }

  if (!matchedOrg) {
    return <Navigate to={ROUTES.ROOT} replace />;
  }

  // Hold the spinner until billing status arrives. Without this, child
  // routes mount briefly while `useOrgBillingStatus` is pending and fire
  // requests that hit `SubscriptionGuard` → 402 before `PaywallScreen`
  // mounts. Most visible after `POST /orgs` lands the user on
  // `/{slug}/onboarding` with the new (Incomplete) org.
  if (!onBillingPath && !billing && billingPending) {
    return (
      <div className='flex h-full w-full items-center justify-center'>
        <Spinner className='size-6' />
      </div>
    );
  }

  if (blocked) {
    const isAdmin = role === "owner" || role === "admin";
    return <PaywallScreen isAdmin={isAdmin} />;
  }

  return (
    <>
      <BillingBanner />
      <Outlet />
    </>
  );
}
