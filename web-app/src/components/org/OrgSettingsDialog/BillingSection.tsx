import { Sparkles } from "lucide-react";
import SubscriptionItemsList from "@/components/billing/SubscriptionItemsList";
import { Button } from "@/components/ui/shadcn/button";
import { useBillingInvoices, useCreatePortalSession, useOrgBilling } from "@/hooks/api/billing";
import type { BillingStatusId, Invoice } from "@/services/api/billing";
import type { Organization } from "@/types/organization";

interface BillingSectionProps {
  org: Organization;
  onClose?: () => void;
}

export default function BillingSection({ org }: BillingSectionProps) {
  const { data: billing, isLoading } = useOrgBilling(org.id);
  const portal = useCreatePortalSession(org.id);
  const { data: invoices } = useBillingInvoices(
    org.id,
    Boolean(billing && billing.status === "active")
  );

  if (isLoading || !billing) {
    return <div className='py-8 text-center text-muted-foreground'>Loading billing…</div>;
  }

  const showPortalButton = billing.status === "active" || billing.status === "past_due";

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

      {invoices && invoices.length > 0 && (
        <section className='space-y-3'>
          <h3 className='font-semibold text-sm'>Invoices</h3>
          <InvoicesTable invoices={invoices} />
        </section>
      )}
    </div>
  );
}

function InvoicesTable({ invoices }: { invoices: Invoice[] }) {
  return (
    <table className='w-full text-sm'>
      <thead className='border-b text-muted-foreground'>
        <tr>
          <th className='px-3 py-2 text-left font-normal text-xs'>Date</th>
          <th className='px-3 py-2 text-left font-normal text-xs'>Due</th>
          <th className='px-3 py-2 text-right font-normal text-xs'>Total</th>
          <th className='px-3 py-2 text-left font-normal text-xs'>Status</th>
          <th className='px-3 py-2 text-right font-normal text-xs'>Actions</th>
        </tr>
      </thead>
      <tbody>
        {invoices.map((inv) => (
          <tr key={inv.id} className='border-b last:border-0'>
            <td className='px-3 py-3 text-sm'>{formatDate(inv.period_start)}</td>
            <td className='px-3 py-3 text-muted-foreground text-sm'>
              {formatDate(inv.period_end)}
            </td>
            <td className='px-3 py-3 text-right text-sm'>
              {formatAmount(inv.amount_paid || inv.amount_due, inv.currency)}
            </td>
            <td className='px-3 py-3 text-sm capitalize'>{inv.status}</td>
            <td className='px-3 py-3 text-right'>
              {inv.hosted_invoice_url ? (
                <a
                  className='text-primary text-sm hover:underline'
                  href={inv.hosted_invoice_url}
                  target='_blank'
                  rel='noreferrer'
                >
                  View
                </a>
              ) : (
                <span className='text-muted-foreground text-sm'>—</span>
              )}
            </td>
          </tr>
        ))}
      </tbody>
    </table>
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

function formatDate(ts: number | null): string {
  if (!ts) return "—";
  return new Date(ts * 1000).toLocaleDateString(undefined, {
    month: "short",
    day: "numeric",
    year: "numeric"
  });
}

function formatDateIso(iso: string): string {
  return new Date(iso).toLocaleDateString(undefined, {
    month: "short",
    day: "numeric",
    year: "numeric"
  });
}

function formatAmount(amountCents: number, currency: string): string {
  return (amountCents / 100).toLocaleString(undefined, {
    style: "currency",
    currency: currency.toUpperCase()
  });
}
