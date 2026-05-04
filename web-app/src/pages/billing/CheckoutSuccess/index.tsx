import { useEffect, useState } from "react";
import { Navigate, useNavigate, useParams, useSearchParams } from "react-router-dom";
import { Button } from "@/components/ui/shadcn/button";
import { Spinner } from "@/components/ui/shadcn/spinner";
import { useCheckoutSession, useOrgBillingStatus } from "@/hooks/api/billing";
import { useOrgs } from "@/hooks/api/organizations";
import ROUTES from "@/libs/utils/routes";

const SUPPORT_MAIL = "mailto:support@oxy.ai";
const POLL_INTERVAL_MS = 2000;
const TIMEOUT_MS = 60_000;

/**
 * Stripe Checkout success landing page. Sequence:
 *   1. Verify the `session_id` query param belongs to this org and has been
 *      paid (server-side call to Stripe — independent of the webhook).
 *   2. Poll billing status until it flips to `active` (the webhook arriving
 *      writes the DB row; we just wait for it).
 *   3. Navigate the user into their workspace via the org dispatcher.
 *
 * Sits inside `OrgGuard` but the path includes `/billing/`, which triggers
 * `OrgGuard`'s existing paywall bypass — so the page renders even while
 * the org is still in `incomplete` state.
 */
export default function CheckoutSuccessPage() {
  const { orgSlug = "" } = useParams<{ orgSlug: string }>();
  const [searchParams] = useSearchParams();
  const sessionId = searchParams.get("session_id") ?? "";
  const navigate = useNavigate();

  const { data: orgs } = useOrgs();
  const matchedOrg = (orgs ?? []).find((o) => o.slug === orgSlug);
  const orgId = matchedOrg?.id ?? "";

  const verify = useCheckoutSession(orgId, sessionId, !!orgId && !!sessionId);
  const verified = verify.data?.paid === true;

  const { data: billing } = useOrgBillingStatus(orgId, !!orgId && verified, {
    refetchInterval: verified ? POLL_INTERVAL_MS : false
  });

  const [timedOut, setTimedOut] = useState(false);
  useEffect(() => {
    if (!verified) return;
    const t = setTimeout(() => setTimedOut(true), TIMEOUT_MS);
    return () => clearTimeout(t);
  }, [verified]);

  useEffect(() => {
    if (billing?.status === "active") {
      navigate(ROUTES.ORG(orgSlug).ROOT, { replace: true });
    }
  }, [billing?.status, orgSlug, navigate]);

  if (!sessionId) {
    return <Navigate to={ROUTES.ORG(orgSlug).ROOT} replace />;
  }

  if (verify.isError) {
    return (
      <Card
        title='We could not verify your payment'
        description='If you completed checkout, please refresh in a minute or contact your account team.'
        cta={
          <Button asChild>
            <a href={SUPPORT_MAIL}>Contact account team</a>
          </Button>
        }
      />
    );
  }

  if (verify.isPending || (verify.isSuccess && !verified)) {
    return (
      <Card
        title={verify.isPending ? "Verifying payment…" : "Payment is still processing"}
        description={
          verify.isPending
            ? "Just a moment while we confirm your checkout with Stripe."
            : "We will activate your workspace as soon as Stripe confirms the payment."
        }
        spinner
      />
    );
  }

  if (timedOut) {
    return (
      <Card
        title='Still finalizing your subscription'
        description='Your payment was confirmed but activation is taking longer than expected. Refresh in a minute, or contact your account team if this persists.'
        cta={
          <div className='flex flex-col gap-2'>
            <Button onClick={() => window.location.reload()}>Refresh</Button>
            <Button asChild variant='outline'>
              <a href={SUPPORT_MAIL}>Contact account team</a>
            </Button>
          </div>
        }
      />
    );
  }

  return (
    <Card
      title='Activating your workspace…'
      description='Payment received. We are setting up your subscription — this usually takes a few seconds.'
      spinner
    />
  );
}

interface CardProps {
  title: string;
  description: string;
  spinner?: boolean;
  cta?: React.ReactNode;
}

function Card({ title, description, spinner, cta }: CardProps) {
  return (
    <div className='flex min-h-[60vh] w-full items-center justify-center px-6 py-16'>
      <div className='w-full max-w-md space-y-6 rounded-2xl border bg-card p-8 text-center'>
        {spinner ? (
          <div className='flex justify-center'>
            <Spinner className='size-6' />
          </div>
        ) : null}
        <div className='space-y-2'>
          <h1 className='font-serif text-2xl tracking-tight'>{title}</h1>
          <p className='text-muted-foreground text-sm'>{description}</p>
        </div>
        {cta ? <div className='space-y-2'>{cta}</div> : null}
      </div>
    </div>
  );
}
