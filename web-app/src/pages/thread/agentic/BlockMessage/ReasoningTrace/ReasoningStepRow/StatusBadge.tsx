import { Check, Loader2 } from "lucide-react";

interface StatusBadgeProps {
  isRunning: boolean;
  isDone: boolean;
  hasError: boolean;
}

const StatusBadge = ({ isRunning, isDone, hasError }: StatusBadgeProps) => (
  <>
    {isRunning && <Loader2 className='h-3 w-3 shrink-0 animate-spin text-primary' />}
    {isDone && <Check className='h-3 w-3 shrink-0 text-primary' />}
    {hasError && <span className='shrink-0 text-destructive text-xs'>Error</span>}
  </>
);

export default StatusBadge;
