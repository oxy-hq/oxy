import { Badge } from "@/components/ui/shadcn/badge";
import type { LocationStatus } from "@/types/operatingGraph";
import { LOCATION_STATUS_LABELS } from "../utils";

/**
 * Open is the state an operator wants, so it carries the primary tint.
 * Pre-launch is the dialog's amber "in progress"; launching sits between the
 * two. Archived and terminated are deliberately flat — nothing to act on.
 */
export function LocationStatusBadge({ status }: { status: LocationStatus }) {
  const label = LOCATION_STATUS_LABELS[status];
  switch (status) {
    case "open":
      return (
        <Badge variant='outline' className='border-primary/30 bg-primary/5 text-primary'>
          {label}
        </Badge>
      );
    case "launching":
      return (
        <Badge variant='outline' className='border-primary/20 text-primary'>
          {label}
        </Badge>
      );
    case "pre_launch":
      return (
        <Badge
          variant='outline'
          className='border-amber-400/40 bg-amber-50 text-amber-700 dark:bg-amber-950/30 dark:text-amber-400'
        >
          {label}
        </Badge>
      );
    default:
      return (
        <Badge variant='outline' className='text-muted-foreground'>
          {label}
        </Badge>
      );
  }
}
