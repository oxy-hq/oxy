import { Check } from "lucide-react";
import { Spinner } from "@/components/ui/shadcn/spinner";

interface StatusBadgeProps {
  isRunning: boolean;
  isDone: boolean;
  hasError: boolean;
}

const StatusBadge = ({ isRunning, isDone, hasError }: StatusBadgeProps) => (
  <>
    {isRunning && <Spinner className='size-3 text-info' />}
    {isDone && <Check className='h-3 w-3 shrink-0 text-status-success-text' />}
    {hasError && <span className='shrink-0 font-medium text-destructive text-xs'>Error</span>}
  </>
);

export default StatusBadge;
