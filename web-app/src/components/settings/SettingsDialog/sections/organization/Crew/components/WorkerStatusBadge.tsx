import { Ban, Lock } from "lucide-react";
import { Badge } from "@/components/ui/shadcn/badge";
import type { WorkerStanding } from "../utils";

/**
 * Active is the quiet default; only the two states an admin should act on
 * carry colour. The amber treatment is the one Members uses for an expired
 * invitation, so "needs attention" reads the same across the dialog.
 */
export function WorkerStatusBadge({ standing }: { standing: WorkerStanding }) {
  if (standing === "suspended") {
    return (
      <Badge
        variant='outline'
        className='gap-1 border-destructive/30 bg-destructive/5 text-destructive'
      >
        <Ban className='h-3 w-3' />
        Suspended
      </Badge>
    );
  }
  if (standing === "locked") {
    return (
      <Badge
        variant='outline'
        className='gap-1 border-amber-400/40 bg-amber-50 text-amber-700 dark:bg-amber-950/30 dark:text-amber-400'
      >
        <Lock className='h-3 w-3' />
        Locked
      </Badge>
    );
  }
  return (
    <Badge variant='outline' className='text-muted-foreground'>
      Active
    </Badge>
  );
}
