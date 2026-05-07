import { PriceTierBreakdown } from "@/components/billing/PriceTierBreakdown";
import type { AdminSubscriptionItem } from "@/services/api/billing";

interface Props {
  items: AdminSubscriptionItem[];
  showItemPeriods?: boolean;
}

export function SubscriptionItemsList({ items, showItemPeriods = false }: Props) {
  if (items.length === 0) {
    return <div className='text-muted-foreground text-sm'>No items.</div>;
  }
  return (
    <ul className='space-y-2'>
      {items.map((item) => (
        <li key={item.id} className='rounded-md border p-3'>
          <div className='flex items-baseline justify-between gap-2'>
            <div className='font-medium text-sm'>{itemTitle(item)}</div>
            <div className='text-muted-foreground text-xs'>×{item.quantity}</div>
          </div>
          <div className='text-muted-foreground text-xs'>
            {item.tiers && item.tiers.length > 0
              ? item.interval
                ? `Tiered / ${item.interval}`
                : "Tiered"
              : `${item.amount_display}${item.interval ? ` / ${item.interval}` : ""}`}
          </div>
          {item.tiers && item.tiers.length > 0 ? (
            <PriceTierBreakdown tiers={item.tiers} currency={item.currency} />
          ) : null}
          {showItemPeriods &&
          (item.current_period_start != null || item.current_period_end != null) ? (
            <div className='mt-1 text-muted-foreground text-xs'>
              Period: {formatUnix(item.current_period_start)} →{" "}
              {formatUnix(item.current_period_end)}
            </div>
          ) : null}
        </li>
      ))}
    </ul>
  );
}

function itemTitle(item: AdminSubscriptionItem) {
  if (item.product_name && item.price_nickname) {
    return `${item.product_name} — ${item.price_nickname}`;
  }
  return item.product_name ?? item.price_nickname ?? item.price_id;
}

function formatUnix(secs: number | null) {
  if (secs == null) return "—";
  return new Date(secs * 1000).toLocaleString();
}
