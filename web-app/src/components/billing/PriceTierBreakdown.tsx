import type { AdminPriceTierDto } from "@/services/api/billing";

interface Props {
  tiers: AdminPriceTierDto[];
  currency: string;
}

export function PriceTierBreakdown({ tiers, currency }: Props) {
  return (
    <div className='mt-3 border-t pt-2'>
      <div className='mb-1 font-medium text-[10px] text-muted-foreground uppercase tracking-wide'>
        Tiers
      </div>
      <div className='grid grid-cols-[1fr_auto_auto] gap-x-3 gap-y-0.5 text-xs tabular-nums'>
        <span className='text-muted-foreground/70'>Quantity</span>
        <span className='text-right text-muted-foreground/70'>Unit price</span>
        <span className='text-right text-muted-foreground/70'>Flat amount</span>
        {tiers.map((tier, idx) => {
          const prevUpTo = idx === 0 ? null : (tiers[idx - 1]?.up_to ?? null);
          return (
            <TierRow
              key={tier.up_to ?? "inf"}
              tier={tier}
              prevUpTo={prevUpTo}
              isFirst={idx === 0}
              currency={currency}
            />
          );
        })}
      </div>
    </div>
  );
}

interface TierRowProps {
  tier: AdminPriceTierDto;
  prevUpTo: number | null;
  isFirst: boolean;
  currency: string;
}

function TierRow({ tier, prevUpTo, isFirst, currency }: TierRowProps) {
  return (
    <>
      <span>{formatTierQuantity(tier.up_to, prevUpTo, isFirst)}</span>
      <span className='text-right'>{formatTierAmount(tier.unit_amount, currency)}</span>
      <span className='text-right'>{formatTierAmount(tier.flat_amount, currency)}</span>
    </>
  );
}

function formatTierQuantity(
  upTo: number | null,
  prevUpTo: number | null,
  isFirst: boolean
): string {
  if (!isFirst && prevUpTo === null) return "—";
  const start = isFirst ? 1 : (prevUpTo as number) + 1;
  if (upTo === null) return `${start}+`;
  return `${start} – ${upTo}`;
}

function formatTierAmount(amountCents: number | null, currency: string): string {
  if (amountCents === null) return "—";
  const value = amountCents / 100;
  return value.toLocaleString(undefined, {
    style: "currency",
    currency: currency.toUpperCase()
  });
}
