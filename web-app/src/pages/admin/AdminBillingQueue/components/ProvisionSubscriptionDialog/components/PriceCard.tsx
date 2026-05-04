import { X } from "lucide-react";
import { Button } from "@/components/ui/shadcn/button";
import { Card } from "@/components/ui/shadcn/card";
import { Checkbox } from "@/components/ui/shadcn/checkbox";
import { Label } from "@/components/ui/shadcn/label";
import type { AdminPriceDto } from "@/services/api/billing";

interface PriceCardProps {
  price: AdminPriceDto;
  isSeatSync: boolean;
  onSeatSyncToggle: () => void;
  onRemove: () => void;
}

export function PriceCard({ price, isSeatSync, onSeatSyncToggle, onRemove }: PriceCardProps) {
  const seatId = `seat-sync-${price.id}`;
  return (
    <Card className='gap-0 p-3'>
      <div className='flex items-start justify-between gap-2'>
        <div className='min-w-0 flex-1'>
          {price.product_name ? (
            <div className='font-medium text-muted-foreground text-xs uppercase tracking-wide'>
              {price.product_name}
            </div>
          ) : null}
          <div className='mt-0.5 truncate font-medium text-sm'>
            {price.nickname ?? "Untitled price"}
          </div>
          <div className='mt-1 text-muted-foreground text-xs tabular-nums'>
            {price.amount_display}{" "}
            <span className='text-muted-foreground/60'>/ {price.interval}</span>
          </div>
        </div>
        <Button
          type='button'
          variant='ghost'
          size='icon'
          onClick={onRemove}
          aria-label='Remove price'
          className='-mt-1 -mr-1 size-7 shrink-0'
        >
          <X className='size-4' />
        </Button>
      </div>

      <div className='mt-3 flex items-center gap-2 border-t pt-3'>
        <Checkbox
          id={seatId}
          checked={isSeatSync}
          onCheckedChange={() => {
            if (!isSeatSync) onSeatSyncToggle();
          }}
        />
        <Label htmlFor={seatId} className='cursor-pointer font-normal text-xs'>
          Seat sync
          <span className='ml-1 text-muted-foreground'>
            (quantity tracks workspace member count)
          </span>
        </Label>
      </div>
    </Card>
  );
}
