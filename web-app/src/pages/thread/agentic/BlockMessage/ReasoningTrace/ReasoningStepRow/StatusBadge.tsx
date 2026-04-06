import { Check } from "lucide-react";
import { Spinner } from "@/components/ui/shadcn/spinner";

interface StatusBadgeProps {
  isRunning: boolean;
  isDone: boolean;
  hasError: boolean;
}

const StatusBadge = ({ isRunning, isDone, hasError }: StatusBadgeProps) => (
  <>
    {isRunning && <Spinner className='size-3 text-primary' />}
    {isDone && <Check className='h-3 w-3 shrink-0 text-primary' />}
    {hasError && <span className='shrink-0 text-destructive text-xs'>Error</span>}
  </>
);

export default StatusBadge;
