import { Sparkles } from "lucide-react";
import { SubscriptionItemsList } from "@/components/billing/SubscriptionItemsList";
import { Button } from "@/components/ui/shadcn/button";
import { useBillingInvoices, useCreatePortalSession, useOrgBilling } from "@/hooks/api/billing";
import type { BillingStatusId } from "@/services/api/billing";
import type { Organization } from "@/types/organization";
import { BillingSkeleton } from "./components/BillingSkeleton";
import { InvoicesSkeleton } from "./components/InvoicesSkeleton";
import { InvoicesTable } from "./components/InvoicesTable";

interface BillingSectionProps {
  org: Organization;
  onClose?: () => void;
}

export default function BillingSection({ org }: BillingSectionProps) {
  const { data: billing, isLoading } = useOrgBilling(org.id);
  const portal = useCreatePortalSession(org.id);
  const isActive = Boolean(billing && billing.status === "active");
  const { data: invoices, isLoading: isLoadingInvoices } = useBillingInvoices(org.id, isActive);

  if (isLoading || !billing) {
    return <BillingSkeleton />;
  }

  const showPortalButton = billing.status === "active" || billing.status === "past_due";
  const hasInvoices = invoices && invoices.length > 0;
  const showInvoicesSection = isActive && (isLoadingInvoices || hasInvoices);

  return (
    <div className='space-y-8'>
      <section className='flex items-start justify-between gap-4'>
        <div className='flex items-start gap-3'>
          <Sparkles className='mt-1 size-7 text-foreground/80' strokeWidth={1.25} />
          <div>
            <h3 className='font-semibold text-base'>{statusLabel(billing.status)}</h3>
            <p className='text-muted-foreground text-sm'>{seatLine(billing)}</p>
            <p className='mt-1 text-muted-foreground text-xs'>{statusLine(billing)}</p>
          </div>
        </div>
        {showPortalButton ? (
          <Button variant='outline' onClick={() => portal.mutate()} disabled={portal.isPending}>
            {portal.isPending ? "Redirecting…" : "Update payment method"}
          </Button>
        ) : null}
      </section>

      {billing.status === "active" && billing.items.length > 0 ? (
        <section className='space-y-3'>
          <SubscriptionItemsList items={billing.items} />
        </section>
      ) : null}

      {billing.payment_action_url ? (
        <section>
          <Button asChild>
            <a href={billing.payment_action_url} target='_blank' rel='noreferrer'>
              Complete payment
            </a>
          </Button>
        </section>
      ) : null}

      {showInvoicesSection ? (
        <section className='space-y-3'>
          <h3 className='font-semibold text-sm'>Invoices</h3>
          {isLoadingInvoices ? <InvoicesSkeleton /> : <InvoicesTable invoices={invoices ?? []} />}
        </section>
      ) : null}
    </div>
  );
}

function statusLabel(s: BillingStatusId): string {
  switch (s) {
    case "active":
      return "Subscription active";
    case "past_due":
      return "Payment past due";
    case "unpaid":
      return "Subscription unpaid";
    case "canceled":
      return "Subscription canceled";
    case "incomplete":
      return "Subscription pending";
    default:
      return "Subscription";
  }
}

function seatLine(billing: { seats_used: number; seats_paid: number }) {
  const seats = `${billing.seats_used} active member${billing.seats_used === 1 ? "" : "s"}`;
  if (billing.seats_paid === 0) {
    return `${seats} · no active subscription`;
  }
  return `${seats} · billing for ${billing.seats_paid} seat${billing.seats_paid === 1 ? "" : "s"}`;
}

function statusLine(billing: {
  status: BillingStatusId;
  billing_cycle: string | null;
  current_period_end: string | null;
  grace_period_ends_at: string | null;
}): string {
  if (billing.status === "past_due") {
    const until = billing.grace_period_ends_at ? formatDateIso(billing.grace_period_ends_at) : null;
    return until
      ? `Payment failed. Update your card before ${until} to avoid interruption.`
      : "Payment failed. Update your card to avoid interruption.";
  }
  if (billing.status === "canceled") {
    return "Contact your account team to re-provision access.";
  }
  if (billing.status === "unpaid") {
    return "Update payment method or contact your account team.";
  }
  if (billing.status === "incomplete") {
    return "Pending sales review. Our team will reach out shortly.";
  }
  if (billing.current_period_end) {
    const date = formatDateIso(billing.current_period_end);
    const cycle = billing.billing_cycle === "annual" ? "annually" : "monthly";
    return `Billed ${cycle}. Renews on ${date}.`;
  }
  return "";
}

function formatDateIso(iso: string): string {
  return new Date(iso).toLocaleDateString(undefined, {
    month: "short",
    day: "numeric",
    year: "numeric"
  });
}
