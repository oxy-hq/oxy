import type { Invoice } from "@/services/api/billing";
import { InvoicesTableHeader } from "./InvoicesTableHeader";

export function InvoicesTable({ invoices }: { invoices: Invoice[] }) {
  return (
    <table className='w-full text-sm'>
      <InvoicesTableHeader />
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

function formatDate(ts: number | null): string {
  if (!ts) return "—";
  return new Date(ts * 1000).toLocaleDateString(undefined, {
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
