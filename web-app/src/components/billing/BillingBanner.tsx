import { Button } from "@/components/ui/shadcn/button";
import { useCreatePortalSession, useOrgBillingStatus } from "@/hooks/api/billing";
import useCurrentOrg from "@/stores/useCurrentOrg";

// Top-of-app banner shown only during the `past_due` grace window. After
// grace expires the org is paywalled by `PaywallScreen` instead. When Stripe
// surfaces a `payment_action_required` event (SCA / 3DS), the banner CTA
// links to the hosted invoice page so the customer can re-authenticate.
//
// Reads the member-readable `/billing/status` so it renders for non-admin
// members too. The "Manage billing" portal CTA is admin-only — the portal
// session endpoint requires admin and members would only get a 403.
export function BillingBanner() {
  const orgId = useCurrentOrg((s) => s.org?.id);
  const role = useCurrentOrg((s) => s.role);
  const isAdmin = role === "owner" || role === "admin";
  const { data: billing } = useOrgBillingStatus(orgId ?? "", Boolean(orgId));
  const portal = useCreatePortalSession(orgId ?? "");

  if (!billing) return null;
  if (billing.status !== "past_due") return null;

  const message = billing.grace_period_ends_at
    ? `Payment failed — update your card by ${new Date(billing.grace_period_ends_at).toLocaleDateString()} to keep access.`
    : "Payment failed — update your card to keep access.";

  return (
    <div className='sticky top-0 z-40 flex items-center justify-between gap-4 border-b bg-warning/10 px-4 py-2 text-sm text-warning-foreground'>
      <span>{message}</span>
      {billing.payment_action_url ? (
        <Button asChild size='sm'>
          <a href={billing.payment_action_url} target='_blank' rel='noreferrer'>
            Complete payment
          </a>
        </Button>
      ) : isAdmin ? (
        <Button
          size='sm'
          variant='outline'
          onClick={() => portal.mutate()}
          disabled={portal.isPending}
        >
          Manage billing
        </Button>
      ) : null}
    </div>
  );
}
