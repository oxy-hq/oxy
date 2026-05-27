import type React from "react";
import { Button } from "@/components/ui/shadcn/button";
import { Spinner } from "@/components/ui/shadcn/spinner";
import { cn } from "@/libs/shadcn/utils";

/** Centered loading spinner for a full panel. */
export const LoadingState: React.FC<{ className?: string }> = ({ className }) => (
  <div className={cn("flex h-full items-center justify-center", className)}>
    <Spinner className='h-6 w-6' />
  </div>
);

/** Full-panel error with a retry affordance. */
export const ErrorState: React.FC<{ message?: string; onRetry?: () => void }> = ({
  message = "Something went wrong",
  onRetry
}) => (
  <div className='flex h-full flex-col items-center justify-center gap-2'>
    <p className='text-destructive text-sm'>{message}</p>
    {onRetry && (
      <Button variant='outline' size='sm' onClick={onRetry}>
        Retry
      </Button>
    )}
  </div>
);

/** Empty state — an icon, a headline, and an optional hint or action. */
export const EmptyState: React.FC<{
  icon?: React.ElementType;
  title: string;
  hint?: string;
  action?: React.ReactNode;
  className?: string;
}> = ({ icon: Icon, title, hint, action, className }) => (
  <div
    className={cn(
      "flex h-full flex-col items-center justify-center gap-1.5 text-center text-muted-foreground",
      className
    )}
  >
    {Icon && <Icon className='h-8 w-8 opacity-40' />}
    <p className='font-medium text-foreground text-sm'>{title}</p>
    {hint && <p className='max-w-xs text-xs'>{hint}</p>}
    {action && <div className='mt-2'>{action}</div>}
  </div>
);
