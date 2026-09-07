import { Ban, Check, Clock } from "lucide-react";
import { Badge } from "@/components/ui/shadcn/badge";
import type { KioskState } from "@/libs/frontline";

/**
 * Bound is the state an admin wants, so it is the one that carries the
 * primary tint. Waiting is neutral — nothing is wrong yet. Expired uses the
 * dialog's amber "needs attention"; revoked is deliberately flat.
 */
export function KioskStateBadge({ state }: { state: KioskState }) {
  switch (state) {
    case "bound":
      return (
        <Badge variant='outline' className='gap-1 border-primary/30 bg-primary/5 text-primary'>
          <Check className='h-3 w-3' />
          Bound
        </Badge>
      );
    case "waiting":
      return (
        <Badge variant='outline' className='gap-1 text-muted-foreground'>
          <Clock className='h-3 w-3' />
          Waiting for the tablet
        </Badge>
      );
    case "expired":
      return (
        <Badge
          variant='outline'
          className='gap-1 border-amber-400/40 bg-amber-50 text-amber-700 dark:bg-amber-950/30 dark:text-amber-400'
        >
          <Clock className='h-3 w-3' />
          Link expired
        </Badge>
      );
    case "revoked":
      return (
        <Badge variant='outline' className='gap-1 text-muted-foreground'>
          <Ban className='h-3 w-3' />
          Revoked
        </Badge>
      );
  }
}
